use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::*;
use indoc::{formatdoc, indoc};
use insta::assert_snapshot;
use std::io::BufReader;
use url::Url;

use crate::common::{
    self, build_vendor_links_url, decode_token, download_to_disk, packse_index_url, uv_snapshot,
    venv_bin_path, TestContext,
};
use uv_fs::Simplified;
use uv_static::EnvVars;

#[test]
fn lock_wheel_registry() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
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
        sdist = { url = "https://files.pythonhosted.org/packages/c6/b3/fefbf7e78ab3b805dec67d698dc18dd505af7a18a8dd08868c9b4fa736b5/anyio-3.7.0.tar.gz", hash = "sha256:275d9973793619a5374e1c89a4f4ad3f4b0a5510a2b5b939444bee8f4c4d37ce", size = 142737 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/fe/7ce1926952c8a403b35029e194555558514b365ad77d75125f521a2bec62/anyio-3.7.0-py3-none-any.whl", hash = "sha256:eddca883c4175f14df8aedce21054bfca3adb70ffe76a9f607aef9d7fa2ea7f0", size = 80873 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio", specifier = "==3.7.0" }]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    // Re-run with `--offline`. We shouldn't need a network connection to validate an
    // already-correct lockfile with immutable metadata.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.1
    "###);

    Ok(())
}

/// Lock a requirement from PyPI.
#[test]
fn lock_sdist_registry() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["source-distribution==0.0.1"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "source-distribution" },
        ]

        [package.metadata]
        requires-dist = [{ name = "source-distribution", specifier = "==0.0.1" }]

        [[package]]
        name = "source-distribution"
        version = "0.0.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz", hash = "sha256:1f83ed7498336c7f2ab9b002cf22583d91115ebc624053dc4eb3a45694490106", size = 2157 }
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + source-distribution==0.0.1
    "###);

    Ok(())
}

/// Lock a Git requirement using `tool.uv.sources`.
#[test]
#[cfg(feature = "git")]
fn lock_sdist_git() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["uv-public-pypackage"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sources]
        uv-public-pypackage = { git = "https://github.com/astral-test/uv-public-pypackage", tag = "0.0.1" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "uv-public-pypackage" },
        ]

        [package.metadata]
        requires-dist = [{ name = "uv-public-pypackage", git = "https://github.com/astral-test/uv-public-pypackage?tag=0.0.1" }]

        [[package]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = { git = "https://github.com/astral-test/uv-public-pypackage?tag=0.0.1#0dacfd662c64cb4ceb16e6cf65a157a8b715b979" }
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Re-run with `--offline`. We shouldn't need a network connection to validate an
    // already-correct lockfile with immutable metadata.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@0dacfd662c64cb4ceb16e6cf65a157a8b715b979)
    "###);

    // Re-lock with a precise commit that maps to the same tag.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["uv-public-pypackage"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sources]
        uv-public-pypackage = { git = "https://github.com/astral-test/uv-public-pypackage", rev = "0dacfd662c64cb4ceb16e6cf65a157a8b715b979" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "uv-public-pypackage" },
        ]

        [package.metadata]
        requires-dist = [{ name = "uv-public-pypackage", git = "https://github.com/astral-test/uv-public-pypackage?rev=0dacfd662c64cb4ceb16e6cf65a157a8b715b979" }]

        [[package]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = { git = "https://github.com/astral-test/uv-public-pypackage?rev=0dacfd662c64cb4ceb16e6cf65a157a8b715b979#0dacfd662c64cb4ceb16e6cf65a157a8b715b979" }
        "###
        );
    });

    // Re-lock with a different commit.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["uv-public-pypackage"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sources]
        uv-public-pypackage = { git = "https://github.com/astral-test/uv-public-pypackage", rev = "b270df1a2fb5d012294e9aaf05e7e0bab1e6a389" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "uv-public-pypackage" },
        ]

        [package.metadata]
        requires-dist = [{ name = "uv-public-pypackage", git = "https://github.com/astral-test/uv-public-pypackage?rev=b270df1a2fb5d012294e9aaf05e7e0bab1e6a389" }]

        [[package]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = { git = "https://github.com/astral-test/uv-public-pypackage?rev=b270df1a2fb5d012294e9aaf05e7e0bab1e6a389#b270df1a2fb5d012294e9aaf05e7e0bab1e6a389" }
        "###
        );
    });

    // Re-lock with a different tag (which matches the new commit).
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["uv-public-pypackage"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sources]
        uv-public-pypackage = { git = "https://github.com/astral-test/uv-public-pypackage", tag = "0.0.2" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "uv-public-pypackage" },
        ]

        [package.metadata]
        requires-dist = [{ name = "uv-public-pypackage", git = "https://github.com/astral-test/uv-public-pypackage?tag=0.0.2" }]

        [[package]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = { git = "https://github.com/astral-test/uv-public-pypackage?tag=0.0.2#b270df1a2fb5d012294e9aaf05e7e0bab1e6a389" }
        "###
        );
    });

    Ok(())
}

/// Lock a Git requirement using PEP 508.
#[test]
#[cfg(feature = "git")]
fn lock_sdist_git_subdirectory() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["example-pkg-a @ git+https://github.com/pypa/sample-namespace-packages.git@df7530eeb8fa0cb7dbb8ecb28363e8e36bfa2f45#subdirectory=pkg_resources/pkg_a"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "example-pkg-a"
        version = "1"
        source = { git = "https://github.com/pypa/sample-namespace-packages.git?subdirectory=pkg_resources%2Fpkg_a&rev=df7530eeb8fa0cb7dbb8ecb28363e8e36bfa2f45#df7530eeb8fa0cb7dbb8ecb28363e8e36bfa2f45" }

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "example-pkg-a" },
        ]

        [package.metadata]
        requires-dist = [{ name = "example-pkg-a", git = "https://github.com/pypa/sample-namespace-packages.git?subdirectory=pkg_resources%2Fpkg_a&rev=df7530eeb8fa0cb7dbb8ecb28363e8e36bfa2f45#df7530eeb8fa0cb7dbb8ecb28363e8e36bfa2f45" }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + example-pkg-a==1 (from git+https://github.com/pypa/sample-namespace-packages.git@df7530eeb8fa0cb7dbb8ecb28363e8e36bfa2f45#subdirectory=pkg_resources/pkg_a)
     + project==0.1.0 (from file://[TEMP_DIR]/)
    "###);

    Ok(())
}

/// Lock a Git requirement using PEP 508.
#[test]
#[cfg(feature = "git")]
fn lock_sdist_git_pep508() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage.git@0.0.1"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "uv-public-pypackage" },
        ]

        [package.metadata]
        requires-dist = [{ name = "uv-public-pypackage", git = "https://github.com/astral-test/uv-public-pypackage.git?rev=0.0.1" }]

        [[package]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = { git = "https://github.com/astral-test/uv-public-pypackage.git?rev=0.0.1#0dacfd662c64cb4ceb16e6cf65a157a8b715b979" }
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Re-lock with a precise commit that maps to the same tag.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage.git@0dacfd662c64cb4ceb16e6cf65a157a8b715b979"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "uv-public-pypackage" },
        ]

        [package.metadata]
        requires-dist = [{ name = "uv-public-pypackage", git = "https://github.com/astral-test/uv-public-pypackage.git?rev=0dacfd662c64cb4ceb16e6cf65a157a8b715b979#0dacfd662c64cb4ceb16e6cf65a157a8b715b979" }]

        [[package]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = { git = "https://github.com/astral-test/uv-public-pypackage.git?rev=0dacfd662c64cb4ceb16e6cf65a157a8b715b979#0dacfd662c64cb4ceb16e6cf65a157a8b715b979" }
        "###
        );
    });

    // Re-lock with a different commit.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage.git@b270df1a2fb5d012294e9aaf05e7e0bab1e6a389"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "uv-public-pypackage" },
        ]

        [package.metadata]
        requires-dist = [{ name = "uv-public-pypackage", git = "https://github.com/astral-test/uv-public-pypackage.git?rev=b270df1a2fb5d012294e9aaf05e7e0bab1e6a389#b270df1a2fb5d012294e9aaf05e7e0bab1e6a389" }]

        [[package]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = { git = "https://github.com/astral-test/uv-public-pypackage.git?rev=b270df1a2fb5d012294e9aaf05e7e0bab1e6a389#b270df1a2fb5d012294e9aaf05e7e0bab1e6a389" }
        "###
        );
    });

    // Re-lock with a different tag (which matches the new commit).
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage.git@0.0.2"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "uv-public-pypackage" },
        ]

        [package.metadata]
        requires-dist = [{ name = "uv-public-pypackage", git = "https://github.com/astral-test/uv-public-pypackage.git?rev=0.0.2" }]

        [[package]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = { git = "https://github.com/astral-test/uv-public-pypackage.git?rev=0.0.2#b270df1a2fb5d012294e9aaf05e7e0bab1e6a389" }
        "###
        );
    });

    Ok(())
}

/// Lock a Git requirement using `tool.uv.sources` with a short revision.
#[test]
#[cfg(feature = "git")]
fn lock_sdist_git_short_rev() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["uv-public-pypackage"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sources]
        uv-public-pypackage = { git = "https://github.com/astral-test/uv-public-pypackage", rev = "0dacfd6" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "uv-public-pypackage" },
        ]

        [package.metadata]
        requires-dist = [{ name = "uv-public-pypackage", git = "https://github.com/astral-test/uv-public-pypackage?rev=0dacfd6" }]

        [[package]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = { git = "https://github.com/astral-test/uv-public-pypackage?rev=0dacfd6#0dacfd662c64cb4ceb16e6cf65a157a8b715b979" }
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Re-run with `--offline`. We shouldn't need a network connection to validate an
    // already-correct lockfile with immutable metadata.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@0dacfd662c64cb4ceb16e6cf65a157a8b715b979)
    "###);

    Ok(())
}

/// Lock a requirement from a direct URL to a wheel.
#[test]
fn lock_wheel_url() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio @ https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "4.3.0"
        source = { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "anyio", extras = ["trio"], marker = "extra == 'test'" },
            { name = "coverage", extras = ["toml"], marker = "extra == 'test'", specifier = ">=7" },
            { name = "exceptiongroup", marker = "python_full_version < '3.11'", specifier = ">=1.0.2" },
            { name = "exceptiongroup", marker = "extra == 'test'", specifier = ">=1.2.0" },
            { name = "hypothesis", marker = "extra == 'test'", specifier = ">=4.0" },
            { name = "idna", specifier = ">=2.8" },
            { name = "packaging", marker = "extra == 'doc'" },
            { name = "psutil", marker = "extra == 'test'", specifier = ">=5.9" },
            { name = "pytest", marker = "extra == 'test'", specifier = ">=7.0" },
            { name = "pytest-mock", marker = "extra == 'test'", specifier = ">=3.6.1" },
            { name = "sniffio", specifier = ">=1.1" },
            { name = "sphinx", marker = "extra == 'doc'", specifier = ">=7" },
            { name = "sphinx-autodoc-typehints", marker = "extra == 'doc'", specifier = ">=1.2.0" },
            { name = "sphinx-rtd-theme", marker = "extra == 'doc'" },
            { name = "trio", marker = "extra == 'trio'", specifier = ">=0.23" },
            { name = "trustme", marker = "extra == 'test'" },
            { name = "typing-extensions", marker = "python_full_version < '3.11'", specifier = ">=4.1" },
            { name = "uvloop", marker = "platform_python_implementation == 'CPython' and sys_platform != 'win32' and extra == 'test'", specifier = ">=0.17" },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio", url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl" }]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    // Re-run with `--offline`. This should fail: we need network access to resolve mutable metadata.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to generate package metadata for `anyio==4.3.0 @ direct+https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl`
      Caused by: Network connectivity is disabled, but the requested data wasn't found in the cache for: `https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl`
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 3 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0 (from https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl)
     + idna==3.6
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.1
    "###);

    Ok(())
}

/// Lock a requirement from a direct URL to a source distribution.
#[test]
fn lock_sdist_url() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio @ https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "4.3.0"
        source = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6" }

        [package.metadata]
        requires-dist = [
            { name = "anyio", extras = ["trio"], marker = "extra == 'test'" },
            { name = "coverage", extras = ["toml"], marker = "extra == 'test'", specifier = ">=7" },
            { name = "exceptiongroup", marker = "python_full_version < '3.11'", specifier = ">=1.0.2" },
            { name = "exceptiongroup", marker = "extra == 'test'", specifier = ">=1.2.0" },
            { name = "hypothesis", marker = "extra == 'test'", specifier = ">=4.0" },
            { name = "idna", specifier = ">=2.8" },
            { name = "packaging", marker = "extra == 'doc'" },
            { name = "psutil", marker = "extra == 'test'", specifier = ">=5.9" },
            { name = "pytest", marker = "extra == 'test'", specifier = ">=7.0" },
            { name = "pytest-mock", marker = "extra == 'test'", specifier = ">=3.6.1" },
            { name = "sniffio", specifier = ">=1.1" },
            { name = "sphinx", marker = "extra == 'doc'", specifier = ">=7" },
            { name = "sphinx-autodoc-typehints", marker = "extra == 'doc'", specifier = ">=1.2.0" },
            { name = "sphinx-rtd-theme", marker = "extra == 'doc'" },
            { name = "trio", marker = "extra == 'trio'", specifier = ">=0.23" },
            { name = "trustme", marker = "extra == 'test'" },
            { name = "typing-extensions", marker = "python_full_version < '3.11'", specifier = ">=4.1" },
            { name = "uvloop", marker = "platform_python_implementation == 'CPython' and sys_platform != 'win32' and extra == 'test'", specifier = ">=0.17" },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio", url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz" }]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0 (from https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz)
     + idna==3.6
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.1
    "###);

    Ok(())
}

/// Lock a project with an extra. When resolving, all extras should be included.
#[test]
fn lock_project_extra() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [project.optional-dependencies]
        test = ["iniconfig"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
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
        sdist = { url = "https://files.pythonhosted.org/packages/c6/b3/fefbf7e78ab3b805dec67d698dc18dd505af7a18a8dd08868c9b4fa736b5/anyio-3.7.0.tar.gz", hash = "sha256:275d9973793619a5374e1c89a4f4ad3f4b0a5510a2b5b939444bee8f4c4d37ce", size = 142737 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/fe/7ce1926952c8a403b35029e194555558514b365ad77d75125f521a2bec62/anyio-3.7.0-py3-none-any.whl", hash = "sha256:eddca883c4175f14df8aedce21054bfca3adb70ffe76a9f607aef9d7fa2ea7f0", size = 80873 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio" },
        ]

        [package.optional-dependencies]
        test = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "anyio", specifier = "==3.7.0" },
            { name = "iniconfig", marker = "extra == 'test'" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###);

    // Install the base dependencies from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.1
    "###);

    // Install the extras from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--extra").arg("test"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    Ok(())
}

/// Lock a project with `uv.tool.override-dependencies`.
#[test]
fn lock_project_with_overrides() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["flask==3.0.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv]
        override-dependencies = ["werkzeug==2.3.8"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 9 packages in [TIME]
    "###);

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 9 packages in [TIME]
    "###);

    // Install the base dependencies from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 8 packages in [TIME]
    Installed 8 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.0
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + werkzeug==2.3.8
    "###);

    Ok(())
}

/// Lock a project with `uv.tool.override-dependencies` that reference `tool.uv.sources`.
#[test]
fn lock_project_with_override_sources() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv]
        override-dependencies = ["idna==3.2"]

        [tool.uv.sources]
        idna = { url = "https://files.pythonhosted.org/packages/d7/77/ff688d1504cdc4db2a938e2b7b9adee5dd52e34efbd2431051efc9984de9/idna-3.2-py3-none-any.whl" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    // Install the base dependencies from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 3 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==3.7.0
     + idna==3.2 (from https://files.pythonhosted.org/packages/d7/77/ff688d1504cdc4db2a938e2b7b9adee5dd52e34efbd2431051efc9984de9/idna-3.2-py3-none-any.whl)
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.1
    "###);

    Ok(())
}

/// Lock a project with `uv.tool.constraint-dependencies`.
#[test]
fn lock_project_with_constraints() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv]
        constraint-dependencies = ["idna<3.4"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    // Install the base dependencies from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==3.7.0
     + idna==3.3
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.1
    "###);

    Ok(())
}

/// Lock a project with `uv.tool.constraint-dependencies` that reference `tool.uv.sources`.
#[test]
fn lock_project_with_constraint_sources() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv]
        constraint-dependencies = ["idna<3.4"]

        [tool.uv.sources]
        idna = { url = "https://files.pythonhosted.org/packages/d7/77/ff688d1504cdc4db2a938e2b7b9adee5dd52e34efbd2431051efc9984de9/idna-3.2-py3-none-any.whl" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    // Install the base dependencies from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 3 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==3.7.0
     + idna==3.2 (from https://files.pythonhosted.org/packages/d7/77/ff688d1504cdc4db2a938e2b7b9adee5dd52e34efbd2431051efc9984de9/idna-3.2-py3-none-any.whl)
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.1
    "###);

    Ok(())
}

/// Lock a project with a dependency that has an extra.
#[test]
fn lock_dependency_extra() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["flask[dotenv]"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "blinker"
        version = "1.7.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a1/13/6df5fc090ff4e5d246baf1f45fe9e5623aa8565757dfa5bd243f6a545f9e/blinker-1.7.0.tar.gz", hash = "sha256:e6820ff6fa4e4d1d8e2747c2283749c3f547e4fee112b98555cdcdae32996182", size = 28134 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/fa/2a/7f3714cbc6356a0efec525ce7a0613d581072ed6eb53eb7b9754f33db807/blinker-1.7.0-py3-none-any.whl", hash = "sha256:c3f865d4d54db7abc53758a01601cf343fe55b84c1de4e3fa910e420b438d5b9", size = 13068 },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "colorama", marker = "sys_platform == 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941 },
        ]

        [[package]]
        name = "colorama"
        version = "0.4.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d8/53/6f443c9a4a8358a93a6792e2acffb9d9d5cb0a5cfd8802644b7b1c9a02e4/colorama-0.4.6.tar.gz", hash = "sha256:08695f5cb7ed6e0531a20572697297273c47b8cae5a63ffc6d6ed5c201be6e44", size = 27697 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d1/d6/3965ed04c63042e047cb6a3e6ed1a63a35087b6a609aa3a15ed8ac56c221/colorama-0.4.6-py2.py3-none-any.whl", hash = "sha256:4f1d9991f5acc0ca119f9d443620b77f9d6b33703e51011c16baf57afb285fc6", size = 25335 },
        ]

        [[package]]
        name = "flask"
        version = "3.0.2"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "blinker" },
            { name = "click" },
            { name = "itsdangerous" },
            { name = "jinja2" },
            { name = "werkzeug" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/3f/e0/a89e8120faea1edbfca1a9b171cff7f2bf62ec860bbafcb2c2387c0317be/flask-3.0.2.tar.gz", hash = "sha256:822c03f4b799204250a7ee84b1eddc40665395333973dfb9deebfe425fefcb7d", size = 675248 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/93/a6/aa98bfe0eb9b8b15d36cdfd03c8ca86a03968a87f27ce224fb4f766acb23/flask-3.0.2-py3-none-any.whl", hash = "sha256:3232e0e9c850d781933cf0207523d1ece087eb8d87b23777ae38456e2fbe7c6e", size = 101300 },
        ]

        [package.optional-dependencies]
        dotenv = [
            { name = "python-dotenv" },
        ]

        [[package]]
        name = "itsdangerous"
        version = "2.1.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/7f/a1/d3fb83e7a61fa0c0d3d08ad0a94ddbeff3731c05212617dff3a94e097f08/itsdangerous-2.1.2.tar.gz", hash = "sha256:5dbbc68b317e5e42f327f9021763545dc3fc3bfe22e6deb96aaf1fc38874156a", size = 56143 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/5f/447e04e828f47465eeab35b5d408b7ebaaaee207f48b7136c5a7267a30ae/itsdangerous-2.1.2-py3-none-any.whl", hash = "sha256:2c2349112351b88699d8d4b6b075022c0808887cb7ad10069318a8b0bc88db44", size = 15749 },
        ]

        [[package]]
        name = "jinja2"
        version = "3.1.3"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "markupsafe" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b2/5e/3a21abf3cd467d7876045335e681d276ac32492febe6d98ad89562d1a7e1/Jinja2-3.1.3.tar.gz", hash = "sha256:ac8bd6544d4bb2c9792bf3a159e80bba8fda7f07e81bc3aed565432d5925ba90", size = 268261 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/30/6d/6de6be2d02603ab56e72997708809e8a5b0fbfee080735109b40a3564843/Jinja2-3.1.3-py3-none-any.whl", hash = "sha256:7d6d50dd97d52cbc355597bd845fabfbac3f551e1f99619e39a35ce8c370b5fa", size = 133236 },
        ]

        [[package]]
        name = "markupsafe"
        version = "2.1.5"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/87/5b/aae44c6655f3801e81aa3eef09dbbf012431987ba564d7231722f68df02d/MarkupSafe-2.1.5.tar.gz", hash = "sha256:d283d37a890ba4c1ae73ffadf8046435c76e7bc2247bbb63c00bd1a709c6544b", size = 19384 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/53/bd/583bf3e4c8d6a321938c13f49d44024dbe5ed63e0a7ba127e454a66da974/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:8dec4936e9c3100156f8a2dc89c4b88d5c435175ff03413b443469c7c8c5f4d1", size = 18215 },
            { url = "https://files.pythonhosted.org/packages/48/d6/e7cd795fc710292c3af3a06d80868ce4b02bfbbf370b7cee11d282815a2a/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:3c6b973f22eb18a789b1460b4b91bf04ae3f0c4234a0a6aa6b0a92f6f7b951d4", size = 14069 },
            { url = "https://files.pythonhosted.org/packages/51/b5/5d8ec796e2a08fc814a2c7d2584b55f889a55cf17dd1a90f2beb70744e5c/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ac07bad82163452a6884fe8fa0963fb98c2346ba78d779ec06bd7a6262132aee", size = 29452 },
            { url = "https://files.pythonhosted.org/packages/0a/0d/2454f072fae3b5a137c119abf15465d1771319dfe9e4acbb31722a0fff91/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f5dfb42c4604dddc8e4305050aa6deb084540643ed5804d7455b5df8fe16f5e5", size = 28462 },
            { url = "https://files.pythonhosted.org/packages/2d/75/fd6cb2e68780f72d47e6671840ca517bda5ef663d30ada7616b0462ad1e3/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ea3d8a3d18833cf4304cd2fc9cbb1efe188ca9b5efef2bdac7adc20594a0e46b", size = 27869 },
            { url = "https://files.pythonhosted.org/packages/b0/81/147c477391c2750e8fc7705829f7351cf1cd3be64406edcf900dc633feb2/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:d050b3361367a06d752db6ead6e7edeb0009be66bc3bae0ee9d97fb326badc2a", size = 33906 },
            { url = "https://files.pythonhosted.org/packages/8b/ff/9a52b71839d7a256b563e85d11050e307121000dcebc97df120176b3ad93/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:bec0a414d016ac1a18862a519e54b2fd0fc8bbfd6890376898a6c0891dd82e9f", size = 32296 },
            { url = "https://files.pythonhosted.org/packages/88/07/2dc76aa51b481eb96a4c3198894f38b480490e834479611a4053fbf08623/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:58c98fee265677f63a4385256a6d7683ab1832f3ddd1e66fe948d5880c21a169", size = 33038 },
            { url = "https://files.pythonhosted.org/packages/96/0c/620c1fb3661858c0e37eb3cbffd8c6f732a67cd97296f725789679801b31/MarkupSafe-2.1.5-cp312-cp312-win32.whl", hash = "sha256:8590b4ae07a35970728874632fed7bd57b26b0102df2d2b233b6d9d82f6c62ad", size = 16572 },
            { url = "https://files.pythonhosted.org/packages/3f/14/c3554d512d5f9100a95e737502f4a2323a1959f6d0d01e0d0997b35f7b10/MarkupSafe-2.1.5-cp312-cp312-win_amd64.whl", hash = "sha256:823b65d8706e32ad2df51ed89496147a42a2a6e01c13cfb6ffb8b1e92bc910bb", size = 17127 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "flask", extra = ["dotenv"] },
        ]

        [package.metadata]
        requires-dist = [{ name = "flask", extras = ["dotenv"] }]

        [[package]]
        name = "python-dotenv"
        version = "1.0.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bc/57/e84d88dfe0aec03b7a2d4327012c1627ab5f03652216c63d49846d7a6c58/python-dotenv-1.0.1.tar.gz", hash = "sha256:e324ee90a023d808f1959c46bcbc04446a10ced277783dc6ee09987c37ec10ca", size = 39115 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/6a/3e/b68c118422ec867fa7ab88444e1274aa40681c606d59ac27de5a5588f082/python_dotenv-1.0.1-py3-none-any.whl", hash = "sha256:f7b63ef50f1b690dddf550d03497b66d609393b40b564ed0d674909a68ebf16a", size = 19863 },
        ]

        [[package]]
        name = "werkzeug"
        version = "3.0.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "markupsafe" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/0d/cc/ff1904eb5eb4b455e442834dabf9427331ac0fa02853bf83db817a7dd53d/werkzeug-3.0.1.tar.gz", hash = "sha256:507e811ecea72b18a404947aded4b3390e1db8f826b494d76550ef45bb3b1dcc", size = 801436 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c3/fc/254c3e9b5feb89ff5b9076a23218dafbc99c96ac5941e900b71206e6313b/werkzeug-3.0.1-py3-none-any.whl", hash = "sha256:90a285dc0e42ad56b34e696398b8122ee4c681833fb35b8334a095d82c56da10", size = 226669 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 9 packages in [TIME]
    Installed 9 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + python-dotenv==1.0.1
     + werkzeug==3.0.1
    "###);

    Ok(())
}

/// Lock a project with a dependency that has a conditional extra.
#[test]
fn lock_conditional_dependency_extra() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.7"
        dependencies = ["requests", "requests[socks] ; python_version < '3.10'"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    let lockfile = context.temp_dir.join("uv.lock");

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(&lockfile).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.7"
        resolution-markers = [
            "python_full_version >= '3.10'",
            "python_full_version >= '3.8' and python_full_version < '3.10'",
            "python_full_version < '3.8'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "certifi"
        version = "2024.2.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/71/da/e94e26401b62acd6d91df2b52954aceb7f561743aa5ccc32152886c76c96/certifi-2024.2.2.tar.gz", hash = "sha256:0569859f95fc761b18b45ef421b1290a0f65f147e92a1e5eb3e635f9a5e4e66f", size = 164886 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ba/06/a07f096c664aeb9f01624f858c3add0a4e913d6c96257acb4fce61e7de14/certifi-2024.2.2-py3-none-any.whl", hash = "sha256:dc383c07b76109f368f6106eee2b593b04a011ea4d55f652c6ca24a754d1cdd1", size = 163774 },
        ]

        [[package]]
        name = "charset-normalizer"
        version = "3.3.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/63/09/c1bc53dab74b1816a00d8d030de5bf98f724c52c1635e07681d312f20be8/charset-normalizer-3.3.2.tar.gz", hash = "sha256:f30c3cb33b24454a82faecaf01b19c18562b1e89558fb6c56de4d9118a032fd5", size = 104809 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2b/61/095a0aa1a84d1481998b534177c8566fdc50bb1233ea9a0478cd3cc075bd/charset_normalizer-3.3.2-cp310-cp310-macosx_10_9_universal2.whl", hash = "sha256:25baf083bf6f6b341f4121c2f3c548875ee6f5339300e08be3f2b2ba1721cdd3", size = 194219 },
            { url = "https://files.pythonhosted.org/packages/cc/94/f7cf5e5134175de79ad2059edf2adce18e0685ebdb9227ff0139975d0e93/charset_normalizer-3.3.2-cp310-cp310-macosx_10_9_x86_64.whl", hash = "sha256:06435b539f889b1f6f4ac1758871aae42dc3a8c0e24ac9e60c2384973ad73027", size = 122521 },
            { url = "https://files.pythonhosted.org/packages/46/6a/d5c26c41c49b546860cc1acabdddf48b0b3fb2685f4f5617ac59261b44ae/charset_normalizer-3.3.2-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:9063e24fdb1e498ab71cb7419e24622516c4a04476b17a2dab57e8baa30d6e03", size = 120383 },
            { url = "https://files.pythonhosted.org/packages/b8/60/e2f67915a51be59d4539ed189eb0a2b0d292bf79270410746becb32bc2c3/charset_normalizer-3.3.2-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:6897af51655e3691ff853668779c7bad41579facacf5fd7253b0133308cf000d", size = 138223 },
            { url = "https://files.pythonhosted.org/packages/05/8c/eb854996d5fef5e4f33ad56927ad053d04dc820e4a3d39023f35cad72617/charset_normalizer-3.3.2-cp310-cp310-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:1d3193f4a680c64b4b6a9115943538edb896edc190f0b222e73761716519268e", size = 148101 },
            { url = "https://files.pythonhosted.org/packages/f6/93/bb6cbeec3bf9da9b2eba458c15966658d1daa8b982c642f81c93ad9b40e1/charset_normalizer-3.3.2-cp310-cp310-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:cd70574b12bb8a4d2aaa0094515df2463cb429d8536cfb6c7ce983246983e5a6", size = 140699 },
            { url = "https://files.pythonhosted.org/packages/da/f1/3702ba2a7470666a62fd81c58a4c40be00670e5006a67f4d626e57f013ae/charset_normalizer-3.3.2-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:8465322196c8b4d7ab6d1e049e4c5cb460d0394da4a27d23cc242fbf0034b6b5", size = 142065 },
            { url = "https://files.pythonhosted.org/packages/3f/ba/3f5e7be00b215fa10e13d64b1f6237eb6ebea66676a41b2bcdd09fe74323/charset_normalizer-3.3.2-cp310-cp310-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:a9a8e9031d613fd2009c182b69c7b2c1ef8239a0efb1df3f7c8da66d5dd3d537", size = 144505 },
            { url = "https://files.pythonhosted.org/packages/33/c3/3b96a435c5109dd5b6adc8a59ba1d678b302a97938f032e3770cc84cd354/charset_normalizer-3.3.2-cp310-cp310-musllinux_1_1_aarch64.whl", hash = "sha256:beb58fe5cdb101e3a055192ac291b7a21e3b7ef4f67fa1d74e331a7f2124341c", size = 139425 },
            { url = "https://files.pythonhosted.org/packages/43/05/3bf613e719efe68fb3a77f9c536a389f35b95d75424b96b426a47a45ef1d/charset_normalizer-3.3.2-cp310-cp310-musllinux_1_1_i686.whl", hash = "sha256:e06ed3eb3218bc64786f7db41917d4e686cc4856944f53d5bdf83a6884432e12", size = 145287 },
            { url = "https://files.pythonhosted.org/packages/58/78/a0bc646900994df12e07b4ae5c713f2b3e5998f58b9d3720cce2aa45652f/charset_normalizer-3.3.2-cp310-cp310-musllinux_1_1_ppc64le.whl", hash = "sha256:2e81c7b9c8979ce92ed306c249d46894776a909505d8f5a4ba55b14206e3222f", size = 149929 },
            { url = "https://files.pythonhosted.org/packages/eb/5c/97d97248af4920bc68687d9c3b3c0f47c910e21a8ff80af4565a576bd2f0/charset_normalizer-3.3.2-cp310-cp310-musllinux_1_1_s390x.whl", hash = "sha256:572c3763a264ba47b3cf708a44ce965d98555f618ca42c926a9c1616d8f34269", size = 141605 },
            { url = "https://files.pythonhosted.org/packages/a8/31/47d018ef89f95b8aded95c589a77c072c55e94b50a41aa99c0a2008a45a4/charset_normalizer-3.3.2-cp310-cp310-musllinux_1_1_x86_64.whl", hash = "sha256:fd1abc0d89e30cc4e02e4064dc67fcc51bd941eb395c502aac3ec19fab46b519", size = 142646 },
            { url = "https://files.pythonhosted.org/packages/ae/d5/4fecf1d58bedb1340a50f165ba1c7ddc0400252d6832ff619c4568b36cc0/charset_normalizer-3.3.2-cp310-cp310-win32.whl", hash = "sha256:3d47fa203a7bd9c5b6cee4736ee84ca03b8ef23193c0d1ca99b5089f72645c73", size = 92846 },
            { url = "https://files.pythonhosted.org/packages/a2/a0/4af29e22cb5942488cf45630cbdd7cefd908768e69bdd90280842e4e8529/charset_normalizer-3.3.2-cp310-cp310-win_amd64.whl", hash = "sha256:10955842570876604d404661fbccbc9c7e684caf432c09c715ec38fbae45ae09", size = 100343 },
            { url = "https://files.pythonhosted.org/packages/68/77/02839016f6fbbf808e8b38601df6e0e66c17bbab76dff4613f7511413597/charset_normalizer-3.3.2-cp311-cp311-macosx_10_9_universal2.whl", hash = "sha256:802fe99cca7457642125a8a88a084cef28ff0cf9407060f7b93dca5aa25480db", size = 191647 },
            { url = "https://files.pythonhosted.org/packages/3e/33/21a875a61057165e92227466e54ee076b73af1e21fe1b31f1e292251aa1e/charset_normalizer-3.3.2-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:573f6eac48f4769d667c4442081b1794f52919e7edada77495aaed9236d13a96", size = 121434 },
            { url = "https://files.pythonhosted.org/packages/dd/51/68b61b90b24ca35495956b718f35a9756ef7d3dd4b3c1508056fa98d1a1b/charset_normalizer-3.3.2-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:549a3a73da901d5bc3ce8d24e0600d1fa85524c10287f6004fbab87672bf3e1e", size = 118979 },
            { url = "https://files.pythonhosted.org/packages/e4/a6/7ee57823d46331ddc37dd00749c95b0edec2c79b15fc0d6e6efb532e89ac/charset_normalizer-3.3.2-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:f27273b60488abe721a075bcca6d7f3964f9f6f067c8c4c605743023d7d3944f", size = 136582 },
            { url = "https://files.pythonhosted.org/packages/74/f1/0d9fe69ac441467b737ba7f48c68241487df2f4522dd7246d9426e7c690e/charset_normalizer-3.3.2-cp311-cp311-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:1ceae2f17a9c33cb48e3263960dc5fc8005351ee19db217e9b1bb15d28c02574", size = 146645 },
            { url = "https://files.pythonhosted.org/packages/05/31/e1f51c76db7be1d4aef220d29fbfa5dbb4a99165d9833dcbf166753b6dc0/charset_normalizer-3.3.2-cp311-cp311-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:65f6f63034100ead094b8744b3b97965785388f308a64cf8d7c34f2f2e5be0c4", size = 139398 },
            { url = "https://files.pythonhosted.org/packages/40/26/f35951c45070edc957ba40a5b1db3cf60a9dbb1b350c2d5bef03e01e61de/charset_normalizer-3.3.2-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:753f10e867343b4511128c6ed8c82f7bec3bd026875576dfd88483c5c73b2fd8", size = 140273 },
            { url = "https://files.pythonhosted.org/packages/07/07/7e554f2bbce3295e191f7e653ff15d55309a9ca40d0362fcdab36f01063c/charset_normalizer-3.3.2-cp311-cp311-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:4a78b2b446bd7c934f5dcedc588903fb2f5eec172f3d29e52a9096a43722adfc", size = 142577 },
            { url = "https://files.pythonhosted.org/packages/d8/b5/eb705c313100defa57da79277d9207dc8d8e45931035862fa64b625bfead/charset_normalizer-3.3.2-cp311-cp311-musllinux_1_1_aarch64.whl", hash = "sha256:e537484df0d8f426ce2afb2d0f8e1c3d0b114b83f8850e5f2fbea0e797bd82ae", size = 137747 },
            { url = "https://files.pythonhosted.org/packages/19/28/573147271fd041d351b438a5665be8223f1dd92f273713cb882ddafe214c/charset_normalizer-3.3.2-cp311-cp311-musllinux_1_1_i686.whl", hash = "sha256:eb6904c354526e758fda7167b33005998fb68c46fbc10e013ca97f21ca5c8887", size = 143375 },
            { url = "https://files.pythonhosted.org/packages/cf/7c/f3b682fa053cc21373c9a839e6beba7705857075686a05c72e0f8c4980ca/charset_normalizer-3.3.2-cp311-cp311-musllinux_1_1_ppc64le.whl", hash = "sha256:deb6be0ac38ece9ba87dea880e438f25ca3eddfac8b002a2ec3d9183a454e8ae", size = 148474 },
            { url = "https://files.pythonhosted.org/packages/1e/49/7ab74d4ac537ece3bc3334ee08645e231f39f7d6df6347b29a74b0537103/charset_normalizer-3.3.2-cp311-cp311-musllinux_1_1_s390x.whl", hash = "sha256:4ab2fe47fae9e0f9dee8c04187ce5d09f48eabe611be8259444906793ab7cbce", size = 140232 },
            { url = "https://files.pythonhosted.org/packages/2d/dc/9dacba68c9ac0ae781d40e1a0c0058e26302ea0660e574ddf6797a0347f7/charset_normalizer-3.3.2-cp311-cp311-musllinux_1_1_x86_64.whl", hash = "sha256:80402cd6ee291dcb72644d6eac93785fe2c8b9cb30893c1af5b8fdd753b9d40f", size = 140859 },
            { url = "https://files.pythonhosted.org/packages/6c/c2/4a583f800c0708dd22096298e49f887b49d9746d0e78bfc1d7e29816614c/charset_normalizer-3.3.2-cp311-cp311-win32.whl", hash = "sha256:7cd13a2e3ddeed6913a65e66e94b51d80a041145a026c27e6bb76c31a853c6ab", size = 92509 },
            { url = "https://files.pythonhosted.org/packages/57/ec/80c8d48ac8b1741d5b963797b7c0c869335619e13d4744ca2f67fc11c6fc/charset_normalizer-3.3.2-cp311-cp311-win_amd64.whl", hash = "sha256:663946639d296df6a2bb2aa51b60a2454ca1cb29835324c640dafb5ff2131a77", size = 99870 },
            { url = "https://files.pythonhosted.org/packages/d1/b2/fcedc8255ec42afee97f9e6f0145c734bbe104aac28300214593eb326f1d/charset_normalizer-3.3.2-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:0b2b64d2bb6d3fb9112bafa732def486049e63de9618b5843bcdd081d8144cd8", size = 192892 },
            { url = "https://files.pythonhosted.org/packages/2e/7d/2259318c202f3d17f3fe6438149b3b9e706d1070fe3fcbb28049730bb25c/charset_normalizer-3.3.2-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:ddbb2551d7e0102e7252db79ba445cdab71b26640817ab1e3e3648dad515003b", size = 122213 },
            { url = "https://files.pythonhosted.org/packages/3a/52/9f9d17c3b54dc238de384c4cb5a2ef0e27985b42a0e5cc8e8a31d918d48d/charset_normalizer-3.3.2-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:55086ee1064215781fff39a1af09518bc9255b50d6333f2e4c74ca09fac6a8f6", size = 119404 },
            { url = "https://files.pythonhosted.org/packages/99/b0/9c365f6d79a9f0f3c379ddb40a256a67aa69c59609608fe7feb6235896e1/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:8f4a014bc36d3c57402e2977dada34f9c12300af536839dc38c0beab8878f38a", size = 137275 },
            { url = "https://files.pythonhosted.org/packages/91/33/749df346e93d7a30cdcb90cbfdd41a06026317bfbfb62cd68307c1a3c543/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:a10af20b82360ab00827f916a6058451b723b4e65030c5a18577c8b2de5b3389", size = 147518 },
            { url = "https://files.pythonhosted.org/packages/72/1a/641d5c9f59e6af4c7b53da463d07600a695b9824e20849cb6eea8a627761/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:8d756e44e94489e49571086ef83b2bb8ce311e730092d2c34ca8f7d925cb20aa", size = 140182 },
            { url = "https://files.pythonhosted.org/packages/ee/fb/14d30eb4956408ee3ae09ad34299131fb383c47df355ddb428a7331cfa1e/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:90d558489962fd4918143277a773316e56c72da56ec7aa3dc3dbbe20fdfed15b", size = 141869 },
            { url = "https://files.pythonhosted.org/packages/df/3e/a06b18788ca2eb6695c9b22325b6fde7dde0f1d1838b1792a0076f58fe9d/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:6ac7ffc7ad6d040517be39eb591cac5ff87416c2537df6ba3cba3bae290c0fed", size = 144042 },
            { url = "https://files.pythonhosted.org/packages/45/59/3d27019d3b447a88fe7e7d004a1e04be220227760264cc41b405e863891b/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:7ed9e526742851e8d5cc9e6cf41427dfc6068d4f5a3bb03659444b4cabf6bc26", size = 138275 },
            { url = "https://files.pythonhosted.org/packages/7b/ef/5eb105530b4da8ae37d506ccfa25057961b7b63d581def6f99165ea89c7e/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:8bdb58ff7ba23002a4c5808d608e4e6c687175724f54a5dade5fa8c67b604e4d", size = 144819 },
            { url = "https://files.pythonhosted.org/packages/a2/51/e5023f937d7f307c948ed3e5c29c4b7a3e42ed2ee0b8cdf8f3a706089bf0/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_ppc64le.whl", hash = "sha256:6b3251890fff30ee142c44144871185dbe13b11bab478a88887a639655be1068", size = 149415 },
            { url = "https://files.pythonhosted.org/packages/24/9d/2e3ef673dfd5be0154b20363c5cdcc5606f35666544381bee15af3778239/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_s390x.whl", hash = "sha256:b4a23f61ce87adf89be746c8a8974fe1c823c891d8f86eb218bb957c924bb143", size = 141212 },
            { url = "https://files.pythonhosted.org/packages/5b/ae/ce2c12fcac59cb3860b2e2d76dc405253a4475436b1861d95fe75bdea520/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:efcb3f6676480691518c177e3b465bcddf57cea040302f9f4e6e191af91174d4", size = 142167 },
            { url = "https://files.pythonhosted.org/packages/ed/3a/a448bf035dce5da359daf9ae8a16b8a39623cc395a2ffb1620aa1bce62b0/charset_normalizer-3.3.2-cp312-cp312-win32.whl", hash = "sha256:d965bba47ddeec8cd560687584e88cf699fd28f192ceb452d1d7ee807c5597b7", size = 93041 },
            { url = "https://files.pythonhosted.org/packages/b6/7c/8debebb4f90174074b827c63242c23851bdf00a532489fba57fef3416e40/charset_normalizer-3.3.2-cp312-cp312-win_amd64.whl", hash = "sha256:96b02a3dc4381e5494fad39be677abcb5e6634bf7b4fa83a6dd3112607547001", size = 100397 },
            { url = "https://files.pythonhosted.org/packages/4f/d1/d547cc26acdb0cc458b152f79b2679d7422f29d41581e6fa907861e88af1/charset_normalizer-3.3.2-cp37-cp37m-macosx_10_9_x86_64.whl", hash = "sha256:95f2a5796329323b8f0512e09dbb7a1860c46a39da62ecb2324f116fa8fdc85c", size = 118254 },
            { url = "https://files.pythonhosted.org/packages/f6/d3/bfc699ab2c4f9245867060744e8136d359412ff1e5ad93be38a46d160f9d/charset_normalizer-3.3.2-cp37-cp37m-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:c002b4ffc0be611f0d9da932eb0f704fe2602a9a949d1f738e4c34c75b0863d5", size = 133657 },
            { url = "https://files.pythonhosted.org/packages/58/a2/0c63d5d7ffac3104b86631b7f2690058c97bf72d3145c0a9cd4fb90c58c2/charset_normalizer-3.3.2-cp37-cp37m-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:a981a536974bbc7a512cf44ed14938cf01030a99e9b3a06dd59578882f06f985", size = 142965 },
            { url = "https://files.pythonhosted.org/packages/2e/37/9223632af0872c86d8b851787f0edd3fe66be4a5378f51242b25212f8374/charset_normalizer-3.3.2-cp37-cp37m-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:3287761bc4ee9e33561a7e058c72ac0938c4f57fe49a09eae428fd88aafe7bb6", size = 136078 },
            { url = "https://files.pythonhosted.org/packages/c9/7a/6d8767fac16f2c80c7fa9f14e0f53d4638271635c306921844dc0b5fd8a6/charset_normalizer-3.3.2-cp37-cp37m-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:42cb296636fcc8b0644486d15c12376cb9fa75443e00fb25de0b8602e64c1714", size = 136822 },
            { url = "https://files.pythonhosted.org/packages/b2/62/5a5dcb9a71390a9511a253bde19c9c89e0b20118e41080185ea69fb2c209/charset_normalizer-3.3.2-cp37-cp37m-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:0a55554a2fa0d408816b3b5cedf0045f4b8e1a6065aec45849de2d6f3f8e9786", size = 139545 },
            { url = "https://files.pythonhosted.org/packages/f2/0e/e06bc07ef4673e4d24dc461333c254586bb759fdd075031539bab6514d07/charset_normalizer-3.3.2-cp37-cp37m-musllinux_1_1_aarch64.whl", hash = "sha256:c083af607d2515612056a31f0a8d9e0fcb5876b7bfc0abad3ecd275bc4ebc2d5", size = 134128 },
            { url = "https://files.pythonhosted.org/packages/8d/b7/9e95102e9a8cce6654b85770794b582dda2921ec1fd924c10fbcf215ad31/charset_normalizer-3.3.2-cp37-cp37m-musllinux_1_1_i686.whl", hash = "sha256:87d1351268731db79e0f8e745d92493ee2841c974128ef629dc518b937d9194c", size = 140017 },
            { url = "https://files.pythonhosted.org/packages/13/f8/eefae0629fa9260f83b826ee3363e311bb03cfdd518dad1bd10d57cb2d84/charset_normalizer-3.3.2-cp37-cp37m-musllinux_1_1_ppc64le.whl", hash = "sha256:bd8f7df7d12c2db9fab40bdd87a7c09b1530128315d047a086fa3ae3435cb3a8", size = 144367 },
            { url = "https://files.pythonhosted.org/packages/91/95/e2cfa7ce962e6c4b59a44a6e19e541c3a0317e543f0e0923f844e8d7d21d/charset_normalizer-3.3.2-cp37-cp37m-musllinux_1_1_s390x.whl", hash = "sha256:c180f51afb394e165eafe4ac2936a14bee3eb10debc9d9e4db8958fe36afe711", size = 136883 },
            { url = "https://files.pythonhosted.org/packages/a0/b1/4e72ef73d68ebdd4748f2df97130e8428c4625785f2b6ece31f555590c2d/charset_normalizer-3.3.2-cp37-cp37m-musllinux_1_1_x86_64.whl", hash = "sha256:8c622a5fe39a48f78944a87d4fb8a53ee07344641b0562c540d840748571b811", size = 136977 },
            { url = "https://files.pythonhosted.org/packages/c8/ce/09d6845504246d95c7443b8c17d0d3911ec5fdc838c3213e16c5e47dee44/charset_normalizer-3.3.2-cp37-cp37m-win32.whl", hash = "sha256:db364eca23f876da6f9e16c9da0df51aa4f104a972735574842618b8c6d999d4", size = 91300 },
            { url = "https://files.pythonhosted.org/packages/96/fc/0cae31c0f150cd1205a2a208079de865f69a8fd052a98856c40c99e36b3c/charset_normalizer-3.3.2-cp37-cp37m-win_amd64.whl", hash = "sha256:86216b5cee4b06df986d214f664305142d9c76df9b6512be2738aa72a2048f99", size = 98127 },
            { url = "https://files.pythonhosted.org/packages/ef/d4/a1d72a8f6aa754fdebe91b848912025d30ab7dced61e9ed8aabbf791ed65/charset_normalizer-3.3.2-cp38-cp38-macosx_10_9_universal2.whl", hash = "sha256:6463effa3186ea09411d50efc7d85360b38d5f09b870c48e4600f63af490e56a", size = 191415 },
            { url = "https://files.pythonhosted.org/packages/13/82/83c188028b6f38d39538442dd127dc794c602ae6d45d66c469f4063a4c30/charset_normalizer-3.3.2-cp38-cp38-macosx_10_9_x86_64.whl", hash = "sha256:6c4caeef8fa63d06bd437cd4bdcf3ffefe6738fb1b25951440d80dc7df8c03ac", size = 121051 },
            { url = "https://files.pythonhosted.org/packages/16/ea/a9e284aa38cccea06b7056d4cbc7adf37670b1f8a668a312864abf1ff7c6/charset_normalizer-3.3.2-cp38-cp38-macosx_11_0_arm64.whl", hash = "sha256:37e55c8e51c236f95b033f6fb391d7d7970ba5fe7ff453dad675e88cf303377a", size = 119143 },
            { url = "https://files.pythonhosted.org/packages/34/2a/f392457d45e24a0c9bfc012887ed4f3c54bf5d4d05a5deb970ffec4b7fc0/charset_normalizer-3.3.2-cp38-cp38-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:fb69256e180cb6c8a894fee62b3afebae785babc1ee98b81cdf68bbca1987f33", size = 137506 },
            { url = "https://files.pythonhosted.org/packages/be/4d/9e370f8281cec2fcc9452c4d1ac513324c32957c5f70c73dd2fa8442a21a/charset_normalizer-3.3.2-cp38-cp38-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:ae5f4161f18c61806f411a13b0310bea87f987c7d2ecdbdaad0e94eb2e404238", size = 147272 },
            { url = "https://files.pythonhosted.org/packages/33/95/ef68482e4a6adf781fae8d183fb48d6f2be8facb414f49c90ba6a5149cd1/charset_normalizer-3.3.2-cp38-cp38-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:b2b0a0c0517616b6869869f8c581d4eb2dd83a4d79e0ebcb7d373ef9956aeb0a", size = 139734 },
            { url = "https://files.pythonhosted.org/packages/3d/09/d82fe4a34c5f0585f9ea1df090e2a71eb9bb1e469723053e1ee9f57c16f3/charset_normalizer-3.3.2-cp38-cp38-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:45485e01ff4d3630ec0d9617310448a8702f70e9c01906b0d0118bdf9d124cf2", size = 141094 },
            { url = "https://files.pythonhosted.org/packages/81/b2/160893421adfa3c45554fb418e321ed342bb10c0a4549e855b2b2a3699cb/charset_normalizer-3.3.2-cp38-cp38-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:eb00ed941194665c332bf8e078baf037d6c35d7c4f3102ea2d4f16ca94a26dc8", size = 144113 },
            { url = "https://files.pythonhosted.org/packages/9e/ef/cd47a63d3200b232792e361cd67530173a09eb011813478b1c0fb8aa7226/charset_normalizer-3.3.2-cp38-cp38-musllinux_1_1_aarch64.whl", hash = "sha256:2127566c664442652f024c837091890cb1942c30937add288223dc895793f898", size = 138555 },
            { url = "https://files.pythonhosted.org/packages/a8/6f/4ff299b97da2ed6358154b6eb3a2db67da2ae204e53d205aacb18a7e4f34/charset_normalizer-3.3.2-cp38-cp38-musllinux_1_1_i686.whl", hash = "sha256:a50aebfa173e157099939b17f18600f72f84eed3049e743b68ad15bd69b6bf99", size = 144944 },
            { url = "https://files.pythonhosted.org/packages/d1/2f/0d1efd07c74c52b6886c32a3b906fb8afd2fecf448650e73ecb90a5a27f1/charset_normalizer-3.3.2-cp38-cp38-musllinux_1_1_ppc64le.whl", hash = "sha256:4d0d1650369165a14e14e1e47b372cfcb31d6ab44e6e33cb2d4e57265290044d", size = 148925 },
            { url = "https://files.pythonhosted.org/packages/bd/28/7ea29e73eea52c7e15b4b9108d0743fc9e4cc2cdb00d275af1df3d46d360/charset_normalizer-3.3.2-cp38-cp38-musllinux_1_1_s390x.whl", hash = "sha256:923c0c831b7cfcb071580d3f46c4baf50f174be571576556269530f4bbd79d04", size = 140732 },
            { url = "https://files.pythonhosted.org/packages/b3/c1/ebca8e87c714a6a561cfee063f0655f742e54b8ae6e78151f60ba8708b3a/charset_normalizer-3.3.2-cp38-cp38-musllinux_1_1_x86_64.whl", hash = "sha256:06a81e93cd441c56a9b65d8e1d043daeb97a3d0856d177d5c90ba85acb3db087", size = 141288 },
            { url = "https://files.pythonhosted.org/packages/74/20/8923a06f15eb3d7f6a306729360bd58f9ead1dc39bc7ea8831f4b407e4ae/charset_normalizer-3.3.2-cp38-cp38-win32.whl", hash = "sha256:6ef1d82a3af9d3eecdba2321dc1b3c238245d890843e040e41e470ffa64c3e25", size = 92373 },
            { url = "https://files.pythonhosted.org/packages/db/fb/d29e343e7c57bbf1231275939f6e75eb740cd47a9d7cb2c52ffeb62ef869/charset_normalizer-3.3.2-cp38-cp38-win_amd64.whl", hash = "sha256:eb8821e09e916165e160797a6c17edda0679379a4be5c716c260e836e122f54b", size = 99577 },
            { url = "https://files.pythonhosted.org/packages/f7/9d/bcf4a449a438ed6f19790eee543a86a740c77508fbc5ddab210ab3ba3a9a/charset_normalizer-3.3.2-cp39-cp39-macosx_10_9_universal2.whl", hash = "sha256:c235ebd9baae02f1b77bcea61bce332cb4331dc3617d254df3323aa01ab47bd4", size = 194198 },
            { url = "https://files.pythonhosted.org/packages/66/fe/c7d3da40a66a6bf2920cce0f436fa1f62ee28aaf92f412f0bf3b84c8ad6c/charset_normalizer-3.3.2-cp39-cp39-macosx_10_9_x86_64.whl", hash = "sha256:5b4c145409bef602a690e7cfad0a15a55c13320ff7a3ad7ca59c13bb8ba4d45d", size = 122494 },
            { url = "https://files.pythonhosted.org/packages/2a/9d/a6d15bd1e3e2914af5955c8eb15f4071997e7078419328fee93dfd497eb7/charset_normalizer-3.3.2-cp39-cp39-macosx_11_0_arm64.whl", hash = "sha256:68d1f8a9e9e37c1223b656399be5d6b448dea850bed7d0f87a8311f1ff3dabb0", size = 120393 },
            { url = "https://files.pythonhosted.org/packages/3d/85/5b7416b349609d20611a64718bed383b9251b5a601044550f0c8983b8900/charset_normalizer-3.3.2-cp39-cp39-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:22afcb9f253dac0696b5a4be4a1c0f8762f8239e21b99680099abd9b2b1b2269", size = 138331 },
            { url = "https://files.pythonhosted.org/packages/79/66/8946baa705c588521afe10b2d7967300e49380ded089a62d38537264aece/charset_normalizer-3.3.2-cp39-cp39-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:e27ad930a842b4c5eb8ac0016b0a54f5aebbe679340c26101df33424142c143c", size = 148097 },
            { url = "https://files.pythonhosted.org/packages/44/80/b339237b4ce635b4af1c73742459eee5f97201bd92b2371c53e11958392e/charset_normalizer-3.3.2-cp39-cp39-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:1f79682fbe303db92bc2b1136016a38a42e835d932bab5b3b1bfcfbf0640e519", size = 140711 },
            { url = "https://files.pythonhosted.org/packages/98/69/5d8751b4b670d623aa7a47bef061d69c279e9f922f6705147983aa76c3ce/charset_normalizer-3.3.2-cp39-cp39-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:b261ccdec7821281dade748d088bb6e9b69e6d15b30652b74cbbac25e280b796", size = 142251 },
            { url = "https://files.pythonhosted.org/packages/1f/8d/33c860a7032da5b93382cbe2873261f81467e7b37f4ed91e25fed62fd49b/charset_normalizer-3.3.2-cp39-cp39-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:122c7fa62b130ed55f8f285bfd56d5f4b4a5b503609d181f9ad85e55c89f4185", size = 144636 },
            { url = "https://files.pythonhosted.org/packages/c2/65/52aaf47b3dd616c11a19b1052ce7fa6321250a7a0b975f48d8c366733b9f/charset_normalizer-3.3.2-cp39-cp39-musllinux_1_1_aarch64.whl", hash = "sha256:d0eccceffcb53201b5bfebb52600a5fb483a20b61da9dbc885f8b103cbe7598c", size = 139514 },
            { url = "https://files.pythonhosted.org/packages/51/fd/0ee5b1c2860bb3c60236d05b6e4ac240cf702b67471138571dad91bcfed8/charset_normalizer-3.3.2-cp39-cp39-musllinux_1_1_i686.whl", hash = "sha256:9f96df6923e21816da7e0ad3fd47dd8f94b2a5ce594e00677c0013018b813458", size = 145528 },
            { url = "https://files.pythonhosted.org/packages/e1/9c/60729bf15dc82e3aaf5f71e81686e42e50715a1399770bcde1a9e43d09db/charset_normalizer-3.3.2-cp39-cp39-musllinux_1_1_ppc64le.whl", hash = "sha256:7f04c839ed0b6b98b1a7501a002144b76c18fb1c1850c8b98d458ac269e26ed2", size = 149804 },
            { url = "https://files.pythonhosted.org/packages/53/cd/aa4b8a4d82eeceb872f83237b2d27e43e637cac9ffaef19a1321c3bafb67/charset_normalizer-3.3.2-cp39-cp39-musllinux_1_1_s390x.whl", hash = "sha256:34d1c8da1e78d2e001f363791c98a272bb734000fcef47a491c1e3b0505657a8", size = 141708 },
            { url = "https://files.pythonhosted.org/packages/54/7f/cad0b328759630814fcf9d804bfabaf47776816ad4ef2e9938b7e1123d04/charset_normalizer-3.3.2-cp39-cp39-musllinux_1_1_x86_64.whl", hash = "sha256:ff8fa367d09b717b2a17a052544193ad76cd49979c805768879cb63d9ca50561", size = 142708 },
            { url = "https://files.pythonhosted.org/packages/c1/9d/254a2f1bcb0ce9acad838e94ed05ba71a7cb1e27affaa4d9e1ca3958cdb6/charset_normalizer-3.3.2-cp39-cp39-win32.whl", hash = "sha256:aed38f6e4fb3f5d6bf81bfa990a07806be9d83cf7bacef998ab1a9bd660a581f", size = 92830 },
            { url = "https://files.pythonhosted.org/packages/2f/0e/d7303ccae9735ff8ff01e36705ad6233ad2002962e8668a970fc000c5e1b/charset_normalizer-3.3.2-cp39-cp39-win_amd64.whl", hash = "sha256:b01b88d45a6fcb69667cd6d2f7a9aeb4bf53760d7fc536bf679ec94fe9f3ff3d", size = 100376 },
            { url = "https://files.pythonhosted.org/packages/28/76/e6222113b83e3622caa4bb41032d0b1bf785250607392e1b778aca0b8a7d/charset_normalizer-3.3.2-py3-none-any.whl", hash = "sha256:3e4d1f6587322d2788836a99c69062fbb091331ec940e02d12d179c1d53e25fc", size = 48543 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "requests" },
            { name = "requests", extra = ["socks"], marker = "python_full_version < '3.10'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "requests" },
            { name = "requests", extras = ["socks"], marker = "python_full_version < '3.10'" },
        ]

        [[package]]
        name = "pysocks"
        version = "1.7.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bd/11/293dd436aea955d45fc4e8a35b6ae7270f5b8e00b53cf6c024c83b657a11/PySocks-1.7.1.tar.gz", hash = "sha256:3f8804571ebe159c380ac6de37643bb4685970655d3bba243530d6558b799aa0", size = 284429 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/8d/59/b4572118e098ac8e46e399a1dd0f2d85403ce8bbaad9ec79373ed6badaf9/PySocks-1.7.1-py3-none-any.whl", hash = "sha256:2725bd0a9925919b9b51739eea5f9e2bae91e83288108a9ad338b2e3a4435ee5", size = 16725 },
        ]

        [[package]]
        name = "requests"
        version = "2.31.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "certifi" },
            { name = "charset-normalizer" },
            { name = "idna" },
            { name = "urllib3", version = "2.0.7", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version < '3.8'" },
            { name = "urllib3", version = "2.2.1", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version >= '3.8'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/9d/be/10918a2eac4ae9f02f6cfe6414b7a155ccd8f7f9d4380d62fd5b955065c3/requests-2.31.0.tar.gz", hash = "sha256:942c5a758f98d790eaed1a29cb6eefc7ffb0d1cf7af05c3d2791656dbd6ad1e1", size = 110794 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/70/8e/0e2d847013cb52cd35b38c009bb167a1a26b2ce6cd6965bf26b47bc0bf44/requests-2.31.0-py3-none-any.whl", hash = "sha256:58cd2187c01e70e6e26505bca751777aa9f2ee0b7f4300988b709f44e013003f", size = 62574 },
        ]

        [package.optional-dependencies]
        socks = [
            { name = "pysocks", marker = "python_full_version < '3.10'" },
        ]

        [[package]]
        name = "urllib3"
        version = "2.0.7"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version < '3.8'",
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/af/47/b215df9f71b4fdba1025fc05a77db2ad243fa0926755a52c5e71659f4e3c/urllib3-2.0.7.tar.gz", hash = "sha256:c97dfde1f7bd43a71c8d2a58e369e9b2bf692d1334ea9f9cae55add7d0dd0f84", size = 282546 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d2/b2/b157855192a68541a91ba7b2bbcb91f1b4faa51f8bae38d8005c034be524/urllib3-2.0.7-py3-none-any.whl", hash = "sha256:fdb6d215c776278489906c2f8916e6e7d4f5a9b602ccbcfdf7f016fc8da0596e", size = 124213 },
        ]

        [[package]]
        name = "urllib3"
        version = "2.2.1"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.10'",
            "python_full_version >= '3.8' and python_full_version < '3.10'",
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/7a/50/7fd50a27caa0652cd4caf224aa87741ea41d3265ad13f010886167cfcc79/urllib3-2.2.1.tar.gz", hash = "sha256:d0570876c61ab9e520d776c38acbbb5b05a776d3f9ff98a5c8fd5162a444cf19", size = 291020 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a2/73/a68704750a7679d0b6d3ad7aa8d4da8e14e151ae82e6fee774e6e0d05ec8/urllib3-2.2.1-py3-none-any.whl", hash = "sha256:450b20ec296a467077128bff42b73080516e71b56ff59a60a02bef2232c4fa9d", size = 121067 },
        ]
        "###
        );
    });

    // TODO(charlie): This test became non-deterministic in https://github.com/astral-sh/uv/pull/6065.
    // But that fix is correct, and the non-determinism itself is a bug.
    // // Re-run with `--locked`.
    // uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    // success: false
    // exit_code: 2
    // ----- stdout -----
    //
    // ----- stderr -----
    // Resolved 7 packages in [TIME]
    // error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided. To update the lockfile, run `uv lock`.
    // "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + requests==2.31.0
     + urllib3==2.2.1
    "###);

    // Validate that the extra is included on relevant Python versions.
    let context_38 = TestContext::new("3.8");

    fs_err::copy(pyproject_toml, context_38.temp_dir.join("pyproject.toml"))?;
    fs_err::copy(lockfile, context_38.temp_dir.join("uv.lock"))?;

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context_38.filters(), context_38.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + pysocks==1.7.1
     + requests==2.31.0
     + urllib3==2.2.1
    "###);

    Ok(())
}

/// Lock a project with a dependency that requests a non-existent extra.
#[test]
fn lock_dependency_non_existent_extra() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["flask[foo]"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 9 packages in [TIME]
    warning: The package `flask==3.0.2` does not have an extra named `foo`
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "blinker"
        version = "1.7.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a1/13/6df5fc090ff4e5d246baf1f45fe9e5623aa8565757dfa5bd243f6a545f9e/blinker-1.7.0.tar.gz", hash = "sha256:e6820ff6fa4e4d1d8e2747c2283749c3f547e4fee112b98555cdcdae32996182", size = 28134 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/fa/2a/7f3714cbc6356a0efec525ce7a0613d581072ed6eb53eb7b9754f33db807/blinker-1.7.0-py3-none-any.whl", hash = "sha256:c3f865d4d54db7abc53758a01601cf343fe55b84c1de4e3fa910e420b438d5b9", size = 13068 },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "colorama", marker = "sys_platform == 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941 },
        ]

        [[package]]
        name = "colorama"
        version = "0.4.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d8/53/6f443c9a4a8358a93a6792e2acffb9d9d5cb0a5cfd8802644b7b1c9a02e4/colorama-0.4.6.tar.gz", hash = "sha256:08695f5cb7ed6e0531a20572697297273c47b8cae5a63ffc6d6ed5c201be6e44", size = 27697 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d1/d6/3965ed04c63042e047cb6a3e6ed1a63a35087b6a609aa3a15ed8ac56c221/colorama-0.4.6-py2.py3-none-any.whl", hash = "sha256:4f1d9991f5acc0ca119f9d443620b77f9d6b33703e51011c16baf57afb285fc6", size = 25335 },
        ]

        [[package]]
        name = "flask"
        version = "3.0.2"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "blinker" },
            { name = "click" },
            { name = "itsdangerous" },
            { name = "jinja2" },
            { name = "werkzeug" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/3f/e0/a89e8120faea1edbfca1a9b171cff7f2bf62ec860bbafcb2c2387c0317be/flask-3.0.2.tar.gz", hash = "sha256:822c03f4b799204250a7ee84b1eddc40665395333973dfb9deebfe425fefcb7d", size = 675248 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/93/a6/aa98bfe0eb9b8b15d36cdfd03c8ca86a03968a87f27ce224fb4f766acb23/flask-3.0.2-py3-none-any.whl", hash = "sha256:3232e0e9c850d781933cf0207523d1ece087eb8d87b23777ae38456e2fbe7c6e", size = 101300 },
        ]

        [[package]]
        name = "itsdangerous"
        version = "2.1.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/7f/a1/d3fb83e7a61fa0c0d3d08ad0a94ddbeff3731c05212617dff3a94e097f08/itsdangerous-2.1.2.tar.gz", hash = "sha256:5dbbc68b317e5e42f327f9021763545dc3fc3bfe22e6deb96aaf1fc38874156a", size = 56143 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/5f/447e04e828f47465eeab35b5d408b7ebaaaee207f48b7136c5a7267a30ae/itsdangerous-2.1.2-py3-none-any.whl", hash = "sha256:2c2349112351b88699d8d4b6b075022c0808887cb7ad10069318a8b0bc88db44", size = 15749 },
        ]

        [[package]]
        name = "jinja2"
        version = "3.1.3"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "markupsafe" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b2/5e/3a21abf3cd467d7876045335e681d276ac32492febe6d98ad89562d1a7e1/Jinja2-3.1.3.tar.gz", hash = "sha256:ac8bd6544d4bb2c9792bf3a159e80bba8fda7f07e81bc3aed565432d5925ba90", size = 268261 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/30/6d/6de6be2d02603ab56e72997708809e8a5b0fbfee080735109b40a3564843/Jinja2-3.1.3-py3-none-any.whl", hash = "sha256:7d6d50dd97d52cbc355597bd845fabfbac3f551e1f99619e39a35ce8c370b5fa", size = 133236 },
        ]

        [[package]]
        name = "markupsafe"
        version = "2.1.5"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/87/5b/aae44c6655f3801e81aa3eef09dbbf012431987ba564d7231722f68df02d/MarkupSafe-2.1.5.tar.gz", hash = "sha256:d283d37a890ba4c1ae73ffadf8046435c76e7bc2247bbb63c00bd1a709c6544b", size = 19384 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/53/bd/583bf3e4c8d6a321938c13f49d44024dbe5ed63e0a7ba127e454a66da974/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:8dec4936e9c3100156f8a2dc89c4b88d5c435175ff03413b443469c7c8c5f4d1", size = 18215 },
            { url = "https://files.pythonhosted.org/packages/48/d6/e7cd795fc710292c3af3a06d80868ce4b02bfbbf370b7cee11d282815a2a/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:3c6b973f22eb18a789b1460b4b91bf04ae3f0c4234a0a6aa6b0a92f6f7b951d4", size = 14069 },
            { url = "https://files.pythonhosted.org/packages/51/b5/5d8ec796e2a08fc814a2c7d2584b55f889a55cf17dd1a90f2beb70744e5c/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ac07bad82163452a6884fe8fa0963fb98c2346ba78d779ec06bd7a6262132aee", size = 29452 },
            { url = "https://files.pythonhosted.org/packages/0a/0d/2454f072fae3b5a137c119abf15465d1771319dfe9e4acbb31722a0fff91/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f5dfb42c4604dddc8e4305050aa6deb084540643ed5804d7455b5df8fe16f5e5", size = 28462 },
            { url = "https://files.pythonhosted.org/packages/2d/75/fd6cb2e68780f72d47e6671840ca517bda5ef663d30ada7616b0462ad1e3/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ea3d8a3d18833cf4304cd2fc9cbb1efe188ca9b5efef2bdac7adc20594a0e46b", size = 27869 },
            { url = "https://files.pythonhosted.org/packages/b0/81/147c477391c2750e8fc7705829f7351cf1cd3be64406edcf900dc633feb2/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:d050b3361367a06d752db6ead6e7edeb0009be66bc3bae0ee9d97fb326badc2a", size = 33906 },
            { url = "https://files.pythonhosted.org/packages/8b/ff/9a52b71839d7a256b563e85d11050e307121000dcebc97df120176b3ad93/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:bec0a414d016ac1a18862a519e54b2fd0fc8bbfd6890376898a6c0891dd82e9f", size = 32296 },
            { url = "https://files.pythonhosted.org/packages/88/07/2dc76aa51b481eb96a4c3198894f38b480490e834479611a4053fbf08623/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:58c98fee265677f63a4385256a6d7683ab1832f3ddd1e66fe948d5880c21a169", size = 33038 },
            { url = "https://files.pythonhosted.org/packages/96/0c/620c1fb3661858c0e37eb3cbffd8c6f732a67cd97296f725789679801b31/MarkupSafe-2.1.5-cp312-cp312-win32.whl", hash = "sha256:8590b4ae07a35970728874632fed7bd57b26b0102df2d2b233b6d9d82f6c62ad", size = 16572 },
            { url = "https://files.pythonhosted.org/packages/3f/14/c3554d512d5f9100a95e737502f4a2323a1959f6d0d01e0d0997b35f7b10/MarkupSafe-2.1.5-cp312-cp312-win_amd64.whl", hash = "sha256:823b65d8706e32ad2df51ed89496147a42a2a6e01c13cfb6ffb8b1e92bc910bb", size = 17127 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "flask" },
        ]

        [package.metadata]
        requires-dist = [{ name = "flask", extras = ["foo"] }]

        [[package]]
        name = "werkzeug"
        version = "3.0.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "markupsafe" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/0d/cc/ff1904eb5eb4b455e442834dabf9427331ac0fa02853bf83db817a7dd53d/werkzeug-3.0.1.tar.gz", hash = "sha256:507e811ecea72b18a404947aded4b3390e1db8f826b494d76550ef45bb3b1dcc", size = 801436 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c3/fc/254c3e9b5feb89ff5b9076a23218dafbc99c96ac5941e900b71206e6313b/werkzeug-3.0.1-py3-none-any.whl", hash = "sha256:90a285dc0e42ad56b34e696398b8122ee4c681833fb35b8334a095d82c56da10", size = 226669 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 9 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 8 packages in [TIME]
    Installed 8 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + werkzeug==3.0.1
    "###);

    Ok(())
}

/// Show updated dependencies on `lock --upgrade`.
#[test]
fn lock_upgrade_log() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["markupsafe<2", "iniconfig"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "markupsafe"
        version = "1.1.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/b9/2e/64db92e53b86efccfaea71321f597fa2e1b2bd3853d8ce658568f7a13094/MarkupSafe-1.1.1.tar.gz", hash = "sha256:29872e92839765e546828bb7754a68c418d927cd064fd4708fab9fe9c8bb116b", size = 19151 }

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
            { name = "markupsafe" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "iniconfig" },
            { name = "markupsafe", specifier = "<2" },
        ]
        "###
        );
    });

    // Run with `--upgrade`; ensure that nothing is changed.
    uv_snapshot!(context.filters(), context.lock().arg("--upgrade"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    // Modify the `pyproject.toml` to loosen a requirement, drop a requirement, and add a
    // requirement.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["markupsafe", "typing-extensions"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    // Run with `--upgrade`; ensure that the updates are reported.
    uv_snapshot!(context.filters(), context.lock().arg("--upgrade"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Removed iniconfig v2.0.0
    Updated markupsafe v1.1.1 -> v2.1.5
    Added typing-extensions v4.10.0
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "markupsafe"
        version = "2.1.5"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/87/5b/aae44c6655f3801e81aa3eef09dbbf012431987ba564d7231722f68df02d/MarkupSafe-2.1.5.tar.gz", hash = "sha256:d283d37a890ba4c1ae73ffadf8046435c76e7bc2247bbb63c00bd1a709c6544b", size = 19384 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/53/bd/583bf3e4c8d6a321938c13f49d44024dbe5ed63e0a7ba127e454a66da974/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:8dec4936e9c3100156f8a2dc89c4b88d5c435175ff03413b443469c7c8c5f4d1", size = 18215 },
            { url = "https://files.pythonhosted.org/packages/48/d6/e7cd795fc710292c3af3a06d80868ce4b02bfbbf370b7cee11d282815a2a/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:3c6b973f22eb18a789b1460b4b91bf04ae3f0c4234a0a6aa6b0a92f6f7b951d4", size = 14069 },
            { url = "https://files.pythonhosted.org/packages/51/b5/5d8ec796e2a08fc814a2c7d2584b55f889a55cf17dd1a90f2beb70744e5c/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ac07bad82163452a6884fe8fa0963fb98c2346ba78d779ec06bd7a6262132aee", size = 29452 },
            { url = "https://files.pythonhosted.org/packages/0a/0d/2454f072fae3b5a137c119abf15465d1771319dfe9e4acbb31722a0fff91/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f5dfb42c4604dddc8e4305050aa6deb084540643ed5804d7455b5df8fe16f5e5", size = 28462 },
            { url = "https://files.pythonhosted.org/packages/2d/75/fd6cb2e68780f72d47e6671840ca517bda5ef663d30ada7616b0462ad1e3/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ea3d8a3d18833cf4304cd2fc9cbb1efe188ca9b5efef2bdac7adc20594a0e46b", size = 27869 },
            { url = "https://files.pythonhosted.org/packages/b0/81/147c477391c2750e8fc7705829f7351cf1cd3be64406edcf900dc633feb2/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:d050b3361367a06d752db6ead6e7edeb0009be66bc3bae0ee9d97fb326badc2a", size = 33906 },
            { url = "https://files.pythonhosted.org/packages/8b/ff/9a52b71839d7a256b563e85d11050e307121000dcebc97df120176b3ad93/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:bec0a414d016ac1a18862a519e54b2fd0fc8bbfd6890376898a6c0891dd82e9f", size = 32296 },
            { url = "https://files.pythonhosted.org/packages/88/07/2dc76aa51b481eb96a4c3198894f38b480490e834479611a4053fbf08623/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:58c98fee265677f63a4385256a6d7683ab1832f3ddd1e66fe948d5880c21a169", size = 33038 },
            { url = "https://files.pythonhosted.org/packages/96/0c/620c1fb3661858c0e37eb3cbffd8c6f732a67cd97296f725789679801b31/MarkupSafe-2.1.5-cp312-cp312-win32.whl", hash = "sha256:8590b4ae07a35970728874632fed7bd57b26b0102df2d2b233b6d9d82f6c62ad", size = 16572 },
            { url = "https://files.pythonhosted.org/packages/3f/14/c3554d512d5f9100a95e737502f4a2323a1959f6d0d01e0d0997b35f7b10/MarkupSafe-2.1.5-cp312-cp312-win_amd64.whl", hash = "sha256:823b65d8706e32ad2df51ed89496147a42a2a6e01c13cfb6ffb8b1e92bc910bb", size = 17127 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "markupsafe" },
            { name = "typing-extensions" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "markupsafe" },
            { name = "typing-extensions" },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 },
        ]
        "###
        );
    });

    Ok(())
}

/// Show updated dependencies on `lock --upgrade`, with a package that resolves to multiple
/// versions.
#[test]
fn lock_upgrade_log_multi_version() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["markupsafe<2 ; sys_platform != 'win32'", "markupsafe==2.0.0 ; sys_platform == 'win32'"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform != 'win32'",
            "sys_platform == 'win32'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "markupsafe"
        version = "1.1.1"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "sys_platform != 'win32'",
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b9/2e/64db92e53b86efccfaea71321f597fa2e1b2bd3853d8ce658568f7a13094/MarkupSafe-1.1.1.tar.gz", hash = "sha256:29872e92839765e546828bb7754a68c418d927cd064fd4708fab9fe9c8bb116b", size = 19151 }

        [[package]]
        name = "markupsafe"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "sys_platform == 'win32'",
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/67/6a/5b3ed5c122e20c33d2562df06faf895a6b91b0a6b96a4626440ffe1d5c8e/MarkupSafe-2.0.0.tar.gz", hash = "sha256:4fae0677f712ee090721d8b17f412f1cbceefbf0dc180fe91bab3232f38b4527", size = 18466 }

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "markupsafe", version = "1.1.1", source = { registry = "https://pypi.org/simple" }, marker = "sys_platform != 'win32'" },
            { name = "markupsafe", version = "2.0.0", source = { registry = "https://pypi.org/simple" }, marker = "sys_platform == 'win32'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "markupsafe", marker = "sys_platform != 'win32'", specifier = "<2" },
            { name = "markupsafe", marker = "sys_platform == 'win32'", specifier = "==2.0.0" },
        ]
        "###
        );
    });

    // Run with `--upgrade`; ensure that nothing is changed.
    uv_snapshot!(context.filters(), context.lock().arg("--upgrade"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    // Modify the `pyproject.toml` to loosen the requirement.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["markupsafe"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    // Run with `--upgrade`; ensure that `markupsafe` is upgraded.
    uv_snapshot!(context.filters(), context.lock().arg("--upgrade"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Updated markupsafe v1.1.1, v2.0.0 -> v2.1.5
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "markupsafe"
        version = "2.1.5"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/87/5b/aae44c6655f3801e81aa3eef09dbbf012431987ba564d7231722f68df02d/MarkupSafe-2.1.5.tar.gz", hash = "sha256:d283d37a890ba4c1ae73ffadf8046435c76e7bc2247bbb63c00bd1a709c6544b", size = 19384 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/53/bd/583bf3e4c8d6a321938c13f49d44024dbe5ed63e0a7ba127e454a66da974/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:8dec4936e9c3100156f8a2dc89c4b88d5c435175ff03413b443469c7c8c5f4d1", size = 18215 },
            { url = "https://files.pythonhosted.org/packages/48/d6/e7cd795fc710292c3af3a06d80868ce4b02bfbbf370b7cee11d282815a2a/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:3c6b973f22eb18a789b1460b4b91bf04ae3f0c4234a0a6aa6b0a92f6f7b951d4", size = 14069 },
            { url = "https://files.pythonhosted.org/packages/51/b5/5d8ec796e2a08fc814a2c7d2584b55f889a55cf17dd1a90f2beb70744e5c/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ac07bad82163452a6884fe8fa0963fb98c2346ba78d779ec06bd7a6262132aee", size = 29452 },
            { url = "https://files.pythonhosted.org/packages/0a/0d/2454f072fae3b5a137c119abf15465d1771319dfe9e4acbb31722a0fff91/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f5dfb42c4604dddc8e4305050aa6deb084540643ed5804d7455b5df8fe16f5e5", size = 28462 },
            { url = "https://files.pythonhosted.org/packages/2d/75/fd6cb2e68780f72d47e6671840ca517bda5ef663d30ada7616b0462ad1e3/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ea3d8a3d18833cf4304cd2fc9cbb1efe188ca9b5efef2bdac7adc20594a0e46b", size = 27869 },
            { url = "https://files.pythonhosted.org/packages/b0/81/147c477391c2750e8fc7705829f7351cf1cd3be64406edcf900dc633feb2/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:d050b3361367a06d752db6ead6e7edeb0009be66bc3bae0ee9d97fb326badc2a", size = 33906 },
            { url = "https://files.pythonhosted.org/packages/8b/ff/9a52b71839d7a256b563e85d11050e307121000dcebc97df120176b3ad93/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:bec0a414d016ac1a18862a519e54b2fd0fc8bbfd6890376898a6c0891dd82e9f", size = 32296 },
            { url = "https://files.pythonhosted.org/packages/88/07/2dc76aa51b481eb96a4c3198894f38b480490e834479611a4053fbf08623/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:58c98fee265677f63a4385256a6d7683ab1832f3ddd1e66fe948d5880c21a169", size = 33038 },
            { url = "https://files.pythonhosted.org/packages/96/0c/620c1fb3661858c0e37eb3cbffd8c6f732a67cd97296f725789679801b31/MarkupSafe-2.1.5-cp312-cp312-win32.whl", hash = "sha256:8590b4ae07a35970728874632fed7bd57b26b0102df2d2b233b6d9d82f6c62ad", size = 16572 },
            { url = "https://files.pythonhosted.org/packages/3f/14/c3554d512d5f9100a95e737502f4a2323a1959f6d0d01e0d0997b35f7b10/MarkupSafe-2.1.5-cp312-cp312-win_amd64.whl", hash = "sha256:823b65d8706e32ad2df51ed89496147a42a2a6e01c13cfb6ffb8b1e92bc910bb", size = 17127 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "markupsafe" },
        ]

        [package.metadata]
        requires-dist = [{ name = "markupsafe" }]
        "###
        );
    });

    Ok(())
}

/// Respect the locked version in an existing lockfile.
#[test]
fn lock_preference() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig<2"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "1.1.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/23/a2/97899f6bd0e873fed3a7e67ae8d3a08b21799430fb4da15cfedf10d6e2c2/iniconfig-1.1.1.tar.gz", hash = "sha256:bc3af051d7d14b2ee5ef9969666def0cd1a000e121eaea580d4a313df4b37f32", size = 8104 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl", hash = "sha256:011e24c64b7f47f6ebd835bb12a743f2fbe9a26d4cecaa7f53bc4f35ee9da8b3", size = 4990 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig", specifier = "<2" }]
        "###
        );
    });

    // Modify the `pyproject.toml` to loosen the requirement.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    // Ensure that the locked version is still respected.
    //
    // Note that we do not use `deterministic!` here because the results depend on the existing lockfile.
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "1.1.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/23/a2/97899f6bd0e873fed3a7e67ae8d3a08b21799430fb4da15cfedf10d6e2c2/iniconfig-1.1.1.tar.gz", hash = "sha256:bc3af051d7d14b2ee5ef9969666def0cd1a000e121eaea580d4a313df4b37f32", size = 8104 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl", hash = "sha256:011e24c64b7f47f6ebd835bb12a743f2fbe9a26d4cecaa7f53bc4f35ee9da8b3", size = 4990 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig" }]
        "###
        );
    });

    uv_snapshot!(context.filters(), context.lock().arg("--upgrade"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Updated iniconfig v1.1.1 -> v2.0.0
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig" }]
        "###
        );
    });

    Ok(())
}

/// If the user includes `git+` in a `tool.uv.sources` entry, we shouldn't fail.
#[test]
#[cfg(feature = "git")]
fn lock_git_plus_prefix() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["uv-public-pypackage"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sources]
        uv-public-pypackage = { git = "git+https://github.com/astral-test/uv-public-pypackage" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "uv-public-pypackage" },
        ]

        [package.metadata]
        requires-dist = [{ name = "uv-public-pypackage", git = "https://github.com/astral-test/uv-public-pypackage" }]

        [[package]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = { git = "https://github.com/astral-test/uv-public-pypackage#b270df1a2fb5d012294e9aaf05e7e0bab1e6a389" }
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Install from the lockfile, excluding development dependencies.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--no-dev"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@b270df1a2fb5d012294e9aaf05e7e0bab1e6a389)
    "###);

    Ok(())
}

#[test]
#[cfg(feature = "git")]
fn lock_partial_git() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.10"
        dependencies = [
           "anyio @ git+https://github.com/agronholm/anyio@4.6.2 ; python_version < '3.12'",
           "anyio ; python_version >= '3.12'"
        ]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.10"
        resolution-markers = [
            "python_full_version >= '3.12'",
            "python_full_version < '3.12'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.12'",
        ]
        dependencies = [
            { name = "idna", marker = "python_full_version >= '3.12'" },
            { name = "sniffio", marker = "python_full_version >= '3.12'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "anyio"
        version = "4.6.2"
        source = { git = "https://github.com/agronholm/anyio?rev=4.6.2#c4844254e6db0cb804c240ba07405db73d810e0b" }
        resolution-markers = [
            "python_full_version < '3.12'",
        ]
        dependencies = [
            { name = "exceptiongroup", marker = "python_full_version < '3.11'" },
            { name = "idna", marker = "python_full_version < '3.12'" },
            { name = "sniffio", marker = "python_full_version < '3.12'" },
            { name = "typing-extensions", marker = "python_full_version < '3.11'" },
        ]

        [[package]]
        name = "exceptiongroup"
        version = "1.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/8e/1c/beef724eaf5b01bb44b6338c8c3494eff7cab376fab4904cfbbc3585dc79/exceptiongroup-1.2.0.tar.gz", hash = "sha256:91f5c769735f051a4290d52edd0858999b57e5876e9f85937691bd4c9fa3ed68", size = 26264 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b8/9a/5028fd52db10e600f1c4674441b968cf2ea4959085bfb5b99fb1250e5f68/exceptiongroup-1.2.0-py3-none-any.whl", hash = "sha256:4bfd3996ac73b41e9b9628b04e079f193850720ea5945fc96a08633c66912f14", size = 16210 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio", version = "4.3.0", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version >= '3.12'" },
            { name = "anyio", version = "4.6.2", source = { git = "https://github.com/agronholm/anyio?rev=4.6.2#c4844254e6db0cb804c240ba07405db73d810e0b" }, marker = "python_full_version < '3.12'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "anyio", marker = "python_full_version < '3.12'", git = "https://github.com/agronholm/anyio?rev=4.6.2" },
            { name = "anyio", marker = "python_full_version >= '3.12'" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    "###);

    // Install from the lockfile, excluding development dependencies.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--no-dev"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.1
    "###);

    Ok(())
}

/// Respect locked versions with `uv lock`, unless `--upgrade` is passed.
#[test]
#[cfg(feature = "git")]
fn lock_git_sha() -> Result<()> {
    let context = TestContext::new("3.12");

    // Lock against `main`.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@main"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    // Write the lockfile such that it's locked against `main`, but a specific commit that isn't
    // the latest.
    let lock = context.temp_dir.child("uv.lock");
    lock.write_str(r#"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "uv-public-pypackage" },
        ]

        [package.metadata]
        requires-dist = [{ name = "uv-public-pypackage", git = "https://github.com/astral-test/uv-public-pypackage?rev=main" }]

        [[package]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = { git = "https://github.com/astral-test/uv-public-pypackage?rev=main#0dacfd662c64cb4ceb16e6cf65a157a8b715b979" }
    "#)?;

    // Lock against `main`.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@main"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    // The lockfile should be unchanged.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Relock with `--upgrade`.
    uv_snapshot!(context.filters(), context.lock().arg("--upgrade-package").arg("uv-public-pypackage"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    // The lockfile should resolve to `b270df1a2fb5d012294e9aaf05e7e0bab1e6a389`, the latest commit
    // on `main`.
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "uv-public-pypackage" },
        ]

        [package.metadata]
        requires-dist = [{ name = "uv-public-pypackage", git = "https://github.com/astral-test/uv-public-pypackage?rev=main" }]

        [[package]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = { git = "https://github.com/astral-test/uv-public-pypackage?rev=main#b270df1a2fb5d012294e9aaf05e7e0bab1e6a389" }
        "###
        );
    });

    Ok(())
}

/// Lock a requirement from PyPI, respecting the `Requires-Python` metadata.
#[test]
fn lock_requires_python() -> Result<()> {
    let context = TestContext::new("3.12");

    let lockfile = context.temp_dir.join("uv.lock");

    // Require >=3.7, which is incompatible with newer versions of `pygls` (>=1.1.0).
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.7"
        dependencies = ["pygls>=1.1.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies for split (python_full_version >= '3.7' and python_full_version < '3.7.9'):
      ╰─▶ Because the requested Python version (>=3.7) does not satisfy Python>=3.7.9 and pygls>=1.1.0,<=1.2.1 depends on Python>=3.7.9,<4, we can conclude that pygls>=1.1.0,<=1.2.1 cannot be used.
          And because only pygls<=1.3.0 is available, we can conclude that all of:
              pygls>=1.1.0,<1.3.0
              pygls>1.3.0
           cannot be used. (1)

          Because the requested Python version (>=3.7) does not satisfy Python>=3.8 and pygls==1.3.0 depends on Python>=3.8, we can conclude that pygls==1.3.0 cannot be used.
          And because we know from (1) that all of:
              pygls>=1.1.0,<1.3.0
              pygls>1.3.0
           cannot be used, we can conclude that pygls>=1.1.0 cannot be used.
          And because your project depends on pygls>=1.1.0, we can conclude that your project's requirements are unsatisfiable.

          hint: Pre-releases are available for `pygls` in the requested range (e.g., 2.0.0a2), but pre-releases weren't enabled (try: `--prerelease=allow`)

          hint: The `requires-python` value (>=3.7) includes Python versions that are not supported by your dependencies (e.g., pygls>=1.1.0,<=1.2.1 only supports >=3.7.9, <4). Consider using a more restrictive `requires-python` value (like >=3.7.9, <4).
    "###);

    // Require >=3.7, and allow locking to a version of `pygls` that is compatible (==1.0.1).
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.7"
        dependencies = ["pygls"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 17 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(&lockfile).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.7"
        resolution-markers = [
            "python_full_version >= '3.8'",
            "python_full_version >= '3.7.9' and python_full_version < '3.8'",
            "python_full_version >= '3.7.4' and python_full_version < '3.7.9'",
            "python_full_version < '3.7.4'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "attrs"
        version = "23.2.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "importlib-metadata", marker = "python_full_version < '3.8'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/e3/fc/f800d51204003fa8ae392c4e8278f256206e7a919b708eef054f5f4b650d/attrs-23.2.0.tar.gz", hash = "sha256:935dc3b529c262f6cf76e50877d35a4bd3c1de194fd41f47a2b7ae8f19971f30", size = 780820 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e0/44/827b2a91a5816512fcaf3cc4ebc465ccd5d598c45cefa6703fcf4a79018f/attrs-23.2.0-py3-none-any.whl", hash = "sha256:99b87a485a5820b23b879f04c2305b44b951b502fd64be915879d77a7e8fc6f1", size = 60752 },
        ]

        [[package]]
        name = "cattrs"
        version = "23.1.2"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.7.9' and python_full_version < '3.8'",
            "python_full_version >= '3.7.4' and python_full_version < '3.7.9'",
            "python_full_version < '3.7.4'",
        ]
        dependencies = [
            { name = "attrs", marker = "python_full_version < '3.8'" },
            { name = "exceptiongroup", marker = "python_full_version < '3.8'" },
            { name = "typing-extensions", version = "4.7.1", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version < '3.8'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/68/d4/27f9fd840e74d51b6d6a024d39ff495b56ffde71d28eb82758b7b85d0617/cattrs-23.1.2.tar.gz", hash = "sha256:db1c821b8c537382b2c7c66678c3790091ca0275ac486c76f3c8f3920e83c657", size = 39998 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/3a/ba/05df14efaa0624fac6b1510e87f5ce446208d2f6ce50270a89b6268aebfe/cattrs-23.1.2-py3-none-any.whl", hash = "sha256:b2bb14311ac17bed0d58785e5a60f022e5431aca3932e3fc5cc8ed8639de50a4", size = 50845 },
        ]

        [[package]]
        name = "cattrs"
        version = "23.2.3"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.8'",
        ]
        dependencies = [
            { name = "attrs", marker = "python_full_version >= '3.8'" },
            { name = "exceptiongroup", marker = "python_full_version >= '3.8' and python_full_version < '3.11'" },
            { name = "typing-extensions", version = "4.10.0", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version >= '3.8' and python_full_version < '3.11'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/1e/57/c6ccd22658c4bcb3beb3f1c262e1f170cf136e913b122763d0ddd328d284/cattrs-23.2.3.tar.gz", hash = "sha256:a934090d95abaa9e911dac357e3a8699e0b4b14f8529bcc7d2b1ad9d51672b9f", size = 610215 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b3/0d/cd4a4071c7f38385dc5ba91286723b4d1090b87815db48216212c6c6c30e/cattrs-23.2.3-py3-none-any.whl", hash = "sha256:0341994d94971052e9ee70662542699a3162ea1e0c62f7ce1b4a57f563685108", size = 57474 },
        ]

        [[package]]
        name = "exceptiongroup"
        version = "1.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/8e/1c/beef724eaf5b01bb44b6338c8c3494eff7cab376fab4904cfbbc3585dc79/exceptiongroup-1.2.0.tar.gz", hash = "sha256:91f5c769735f051a4290d52edd0858999b57e5876e9f85937691bd4c9fa3ed68", size = 26264 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b8/9a/5028fd52db10e600f1c4674441b968cf2ea4959085bfb5b99fb1250e5f68/exceptiongroup-1.2.0-py3-none-any.whl", hash = "sha256:4bfd3996ac73b41e9b9628b04e079f193850720ea5945fc96a08633c66912f14", size = 16210 },
        ]

        [[package]]
        name = "importlib-metadata"
        version = "6.7.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "typing-extensions", version = "4.7.1", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version < '3.8'" },
            { name = "zipp", marker = "python_full_version < '3.8'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/a3/82/f6e29c8d5c098b6be61460371c2c5591f4a335923639edec43b3830650a4/importlib_metadata-6.7.0.tar.gz", hash = "sha256:1aaf550d4f73e5d6783e7acb77aec43d49da8017410afae93822cc9cca98c4d4", size = 53569 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ff/94/64287b38c7de4c90683630338cf28f129decbba0a44f0c6db35a873c73c4/importlib_metadata-6.7.0-py3-none-any.whl", hash = "sha256:cb52082e659e97afc5dac71e79de97d8681de3aa07ff18578330904a9d18e5b5", size = 22934 },
        ]

        [[package]]
        name = "lsprotocol"
        version = "2023.0.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.7.9' and python_full_version < '3.8'",
        ]
        dependencies = [
            { name = "attrs", marker = "python_full_version >= '3.7.9' and python_full_version < '3.8'" },
            { name = "cattrs", version = "23.1.2", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version >= '3.7.9' and python_full_version < '3.8'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/3e/fe/f7671a4fb28606ff1663bba60aff6af21b1e43a977c74c33db13cb83680f/lsprotocol-2023.0.0.tar.gz", hash = "sha256:c9d92e12a3f4ed9317d3068226592860aab5357d93cf5b2451dc244eee8f35f2", size = 69399 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2d/5b/f18eb1823a4cee9bed70cdcc25eed5a75845367c42e63a79010a7c34f8a7/lsprotocol-2023.0.0-py3-none-any.whl", hash = "sha256:e85fc87ee26c816adca9eb497bb3db1a7c79c477a11563626e712eaccf926a05", size = 70789 },
        ]

        [[package]]
        name = "lsprotocol"
        version = "2023.0.1"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.8'",
            "python_full_version >= '3.7.4' and python_full_version < '3.7.9'",
            "python_full_version < '3.7.4'",
        ]
        dependencies = [
            { name = "attrs", marker = "python_full_version < '3.7.9' or python_full_version >= '3.8'" },
            { name = "cattrs", version = "23.1.2", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version < '3.7.9'" },
            { name = "cattrs", version = "23.2.3", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version >= '3.8'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/9d/f6/6e80484ec078d0b50699ceb1833597b792a6c695f90c645fbaf54b947e6f/lsprotocol-2023.0.1.tar.gz", hash = "sha256:cc5c15130d2403c18b734304339e51242d3018a05c4f7d0f198ad6e0cd21861d", size = 69434 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/8d/37/2351e48cb3309673492d3a8c59d407b75fb6630e560eb27ecd4da03adc9a/lsprotocol-2023.0.1-py3-none-any.whl", hash = "sha256:c75223c9e4af2f24272b14c6375787438279369236cd568f596d4951052a60f2", size = 70826 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "pygls", version = "1.0.1", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version < '3.7.4'" },
            { name = "pygls", version = "1.0.2", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version >= '3.7.4' and python_full_version < '3.7.9'" },
            { name = "pygls", version = "1.2.1", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version >= '3.7.9' and python_full_version < '3.8'" },
            { name = "pygls", version = "1.3.0", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version >= '3.8'" },
        ]

        [package.metadata]
        requires-dist = [{ name = "pygls" }]

        [[package]]
        name = "pygls"
        version = "1.0.1"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version < '3.7.4'",
        ]
        dependencies = [
            { name = "lsprotocol", version = "2023.0.1", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version < '3.7.4'" },
            { name = "typeguard", version = "2.13.3", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version < '3.7.4'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/8e/27/58ff0f76b379fc11a1d03e8d4b4e96fd0abb463d27709a7fb4193bcdbbc4/pygls-1.0.1.tar.gz", hash = "sha256:f3ee98ddbb4690eb5c755bc32ba7e129607f14cbd313575f33d0cea443b78cb2", size = 674546 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/da/9b/4fd77a060068f2f3f46f97ed6ba8762c5a73f11ef0c196cfd34f3a9be878/pygls-1.0.1-py3-none-any.whl", hash = "sha256:adacc96da77598c70f46acfdfd1481d3da90cd54f639f7eee52eb6e4dbd57b55", size = 40367 },
        ]

        [[package]]
        name = "pygls"
        version = "1.0.2"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.7.4' and python_full_version < '3.7.9'",
        ]
        dependencies = [
            { name = "lsprotocol", version = "2023.0.1", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version >= '3.7.4' and python_full_version < '3.7.9'" },
            { name = "typeguard", version = "3.0.2", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version >= '3.7.4' and python_full_version < '3.7.9'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/bb/c4/fc9c817ba0f1ad0fdbe1686bc8211a0dc2390ab11cf7780e9eeed718594b/pygls-1.0.2.tar.gz", hash = "sha256:888ed63d1f650b4fc64d603d73d37545386ec533c0caac921aed80f80ea946a4", size = 674931 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/6d/c6/ed6aa6fb709abf57b1a75b83f7809792b79320247391dda8f04d2a003548/pygls-1.0.2-py3-none-any.whl", hash = "sha256:6d278d29fa6559b0f7a448263c85cb64ec6e9369548b02f1a7944060848b21f9", size = 38654 },
        ]

        [[package]]
        name = "pygls"
        version = "1.2.1"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.7.9' and python_full_version < '3.8'",
        ]
        dependencies = [
            { name = "lsprotocol", version = "2023.0.0", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version >= '3.7.9' and python_full_version < '3.8'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/e6/94/534c11ba5475df09542e48d751a66e0448d52bbbb92cbef5541deef7760d/pygls-1.2.1.tar.gz", hash = "sha256:04f9b9c115b622dcc346fb390289066565343d60245a424eca77cb429b911ed8", size = 45274 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/36/31/3799444d3f072ffca1a35eb02a48f964384cc13f001125e87d9f0748687b/pygls-1.2.1-py3-none-any.whl", hash = "sha256:7dcfcf12b6f15beb606afa46de2ed348b65a279c340ef2242a9a35c22eeafe94", size = 55983 },
        ]

        [[package]]
        name = "pygls"
        version = "1.3.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.8'",
        ]
        dependencies = [
            { name = "cattrs", version = "23.2.3", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version >= '3.8'" },
            { name = "lsprotocol", version = "2023.0.1", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version >= '3.8'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/e9/8d/31b50ac0879464049d744a1ddf00dc6474433eb55d40fa0c8e8510591ad2/pygls-1.3.0.tar.gz", hash = "sha256:1b44ace89c9382437a717534f490eadc6fda7c0c6c16ac1eaaf5568e345e4fb8", size = 45539 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/4e/1e/643070d8f5c851958662e7e5df16d9c3a068a598a7ee7bb2eb8d95b4e5d7/pygls-1.3.0-py3-none-any.whl", hash = "sha256:d4a01414b6ed4e34e7e8fd29b77d3e88c29615df7d0bbff49bf019e15ec04b8f", size = 56031 },
        ]

        [[package]]
        name = "typeguard"
        version = "2.13.3"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version < '3.7.4'",
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/3a/38/c61bfcf62a7b572b5e9363a802ff92559cb427ee963048e1442e3aef7490/typeguard-2.13.3.tar.gz", hash = "sha256:00edaa8da3a133674796cf5ea87d9f4b4c367d77476e185e80251cc13dfbb8c4", size = 40604 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/9a/bb/d43e5c75054e53efce310e79d63df0ac3f25e34c926be5dffb7d283fb2a8/typeguard-2.13.3-py3-none-any.whl", hash = "sha256:5e3e3be01e887e7eafae5af63d1f36c849aaa94e3a0112097312aabfa16284f1", size = 17605 },
        ]

        [[package]]
        name = "typeguard"
        version = "3.0.2"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.7.4' and python_full_version < '3.7.9'",
        ]
        dependencies = [
            { name = "importlib-metadata", marker = "python_full_version >= '3.7.4' and python_full_version < '3.7.9'" },
            { name = "typing-extensions", version = "4.7.1", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version >= '3.7.4' and python_full_version < '3.7.9'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/af/40/3398497c6e6951c92abaf933492d6633e7ac4df0bfc9d81f304b3f977f15/typeguard-3.0.2.tar.gz", hash = "sha256:fee5297fdb28f8e9efcb8142b5ee219e02375509cd77ea9d270b5af826358d5a", size = 58171 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e2/62/7d206b0ac6fcbb163215ecc622a54eb747f85ad86d14bc513a834442d0f6/typeguard-3.0.2-py3-none-any.whl", hash = "sha256:bbe993854385284ab42fd5bd3bee6f6556577ce8b50696d6cb956d704f286c8e", size = 30696 },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.7.1"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.7.9' and python_full_version < '3.8'",
            "python_full_version >= '3.7.4' and python_full_version < '3.7.9'",
            "python_full_version < '3.7.4'",
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/3c/8b/0111dd7d6c1478bf83baa1cab85c686426c7a6274119aceb2bd9d35395ad/typing_extensions-4.7.1.tar.gz", hash = "sha256:b75ddc264f0ba5615db7ba217daeb99701ad295353c45f9e95963337ceeeffb2", size = 72876 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ec/6b/63cc3df74987c36fe26157ee12e09e8f9db4de771e0f3404263117e75b95/typing_extensions-4.7.1-py3-none-any.whl", hash = "sha256:440d5dd3af93b060174bf433bccd69b0babc3b15b1a8dca43789fd7f61514b36", size = 33232 },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.8'",
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 },
        ]

        [[package]]
        name = "zipp"
        version = "3.15.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/00/27/f0ac6b846684cecce1ee93d32450c45ab607f65c2e0255f0092032d91f07/zipp-3.15.0.tar.gz", hash = "sha256:112929ad649da941c23de50f356a2b5570c954b65150642bccdd66bf194d224b", size = 18454 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/5b/fa/c9e82bbe1af6266adf08afb563905eb87cab83fde00a0a08963510621047/zipp-3.15.0-py3-none-any.whl", hash = "sha256:48904fc76a60e542af151aded95726c1a5c34ed43ab4134b597665c86d7ad556", size = 6758 },
        ]
        "###
        );
    });

    // Remove the lockfile.
    fs_err::remove_file(&lockfile)?;

    // Bump the Python requirement, which should allow a newer version of `pygls`.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.7.9"
        dependencies = ["pygls"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 13 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(&lockfile).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.7.9"
        resolution-markers = [
            "python_full_version >= '3.8'",
            "python_full_version < '3.8'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "attrs"
        version = "23.2.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "importlib-metadata", marker = "python_full_version < '3.8'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/e3/fc/f800d51204003fa8ae392c4e8278f256206e7a919b708eef054f5f4b650d/attrs-23.2.0.tar.gz", hash = "sha256:935dc3b529c262f6cf76e50877d35a4bd3c1de194fd41f47a2b7ae8f19971f30", size = 780820 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e0/44/827b2a91a5816512fcaf3cc4ebc465ccd5d598c45cefa6703fcf4a79018f/attrs-23.2.0-py3-none-any.whl", hash = "sha256:99b87a485a5820b23b879f04c2305b44b951b502fd64be915879d77a7e8fc6f1", size = 60752 },
        ]

        [[package]]
        name = "cattrs"
        version = "23.1.2"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version < '3.8'",
        ]
        dependencies = [
            { name = "attrs", marker = "python_full_version < '3.8'" },
            { name = "exceptiongroup", marker = "python_full_version < '3.8'" },
            { name = "typing-extensions", version = "4.7.1", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version < '3.8'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/68/d4/27f9fd840e74d51b6d6a024d39ff495b56ffde71d28eb82758b7b85d0617/cattrs-23.1.2.tar.gz", hash = "sha256:db1c821b8c537382b2c7c66678c3790091ca0275ac486c76f3c8f3920e83c657", size = 39998 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/3a/ba/05df14efaa0624fac6b1510e87f5ce446208d2f6ce50270a89b6268aebfe/cattrs-23.1.2-py3-none-any.whl", hash = "sha256:b2bb14311ac17bed0d58785e5a60f022e5431aca3932e3fc5cc8ed8639de50a4", size = 50845 },
        ]

        [[package]]
        name = "cattrs"
        version = "23.2.3"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.8'",
        ]
        dependencies = [
            { name = "attrs", marker = "python_full_version >= '3.8'" },
            { name = "exceptiongroup", marker = "python_full_version >= '3.8' and python_full_version < '3.11'" },
            { name = "typing-extensions", version = "4.10.0", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version >= '3.8' and python_full_version < '3.11'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/1e/57/c6ccd22658c4bcb3beb3f1c262e1f170cf136e913b122763d0ddd328d284/cattrs-23.2.3.tar.gz", hash = "sha256:a934090d95abaa9e911dac357e3a8699e0b4b14f8529bcc7d2b1ad9d51672b9f", size = 610215 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b3/0d/cd4a4071c7f38385dc5ba91286723b4d1090b87815db48216212c6c6c30e/cattrs-23.2.3-py3-none-any.whl", hash = "sha256:0341994d94971052e9ee70662542699a3162ea1e0c62f7ce1b4a57f563685108", size = 57474 },
        ]

        [[package]]
        name = "exceptiongroup"
        version = "1.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/8e/1c/beef724eaf5b01bb44b6338c8c3494eff7cab376fab4904cfbbc3585dc79/exceptiongroup-1.2.0.tar.gz", hash = "sha256:91f5c769735f051a4290d52edd0858999b57e5876e9f85937691bd4c9fa3ed68", size = 26264 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b8/9a/5028fd52db10e600f1c4674441b968cf2ea4959085bfb5b99fb1250e5f68/exceptiongroup-1.2.0-py3-none-any.whl", hash = "sha256:4bfd3996ac73b41e9b9628b04e079f193850720ea5945fc96a08633c66912f14", size = 16210 },
        ]

        [[package]]
        name = "importlib-metadata"
        version = "6.7.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "typing-extensions", version = "4.7.1", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version < '3.8'" },
            { name = "zipp", marker = "python_full_version < '3.8'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/a3/82/f6e29c8d5c098b6be61460371c2c5591f4a335923639edec43b3830650a4/importlib_metadata-6.7.0.tar.gz", hash = "sha256:1aaf550d4f73e5d6783e7acb77aec43d49da8017410afae93822cc9cca98c4d4", size = 53569 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ff/94/64287b38c7de4c90683630338cf28f129decbba0a44f0c6db35a873c73c4/importlib_metadata-6.7.0-py3-none-any.whl", hash = "sha256:cb52082e659e97afc5dac71e79de97d8681de3aa07ff18578330904a9d18e5b5", size = 22934 },
        ]

        [[package]]
        name = "lsprotocol"
        version = "2023.0.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version < '3.8'",
        ]
        dependencies = [
            { name = "attrs", marker = "python_full_version < '3.8'" },
            { name = "cattrs", version = "23.1.2", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version < '3.8'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/3e/fe/f7671a4fb28606ff1663bba60aff6af21b1e43a977c74c33db13cb83680f/lsprotocol-2023.0.0.tar.gz", hash = "sha256:c9d92e12a3f4ed9317d3068226592860aab5357d93cf5b2451dc244eee8f35f2", size = 69399 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2d/5b/f18eb1823a4cee9bed70cdcc25eed5a75845367c42e63a79010a7c34f8a7/lsprotocol-2023.0.0-py3-none-any.whl", hash = "sha256:e85fc87ee26c816adca9eb497bb3db1a7c79c477a11563626e712eaccf926a05", size = 70789 },
        ]

        [[package]]
        name = "lsprotocol"
        version = "2023.0.1"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.8'",
        ]
        dependencies = [
            { name = "attrs", marker = "python_full_version >= '3.8'" },
            { name = "cattrs", version = "23.2.3", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version >= '3.8'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/9d/f6/6e80484ec078d0b50699ceb1833597b792a6c695f90c645fbaf54b947e6f/lsprotocol-2023.0.1.tar.gz", hash = "sha256:cc5c15130d2403c18b734304339e51242d3018a05c4f7d0f198ad6e0cd21861d", size = 69434 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/8d/37/2351e48cb3309673492d3a8c59d407b75fb6630e560eb27ecd4da03adc9a/lsprotocol-2023.0.1-py3-none-any.whl", hash = "sha256:c75223c9e4af2f24272b14c6375787438279369236cd568f596d4951052a60f2", size = 70826 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "pygls", version = "1.2.1", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version < '3.8'" },
            { name = "pygls", version = "1.3.0", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version >= '3.8'" },
        ]

        [package.metadata]
        requires-dist = [{ name = "pygls" }]

        [[package]]
        name = "pygls"
        version = "1.2.1"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version < '3.8'",
        ]
        dependencies = [
            { name = "lsprotocol", version = "2023.0.0", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version < '3.8'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/e6/94/534c11ba5475df09542e48d751a66e0448d52bbbb92cbef5541deef7760d/pygls-1.2.1.tar.gz", hash = "sha256:04f9b9c115b622dcc346fb390289066565343d60245a424eca77cb429b911ed8", size = 45274 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/36/31/3799444d3f072ffca1a35eb02a48f964384cc13f001125e87d9f0748687b/pygls-1.2.1-py3-none-any.whl", hash = "sha256:7dcfcf12b6f15beb606afa46de2ed348b65a279c340ef2242a9a35c22eeafe94", size = 55983 },
        ]

        [[package]]
        name = "pygls"
        version = "1.3.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.8'",
        ]
        dependencies = [
            { name = "cattrs", version = "23.2.3", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version >= '3.8'" },
            { name = "lsprotocol", version = "2023.0.1", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version >= '3.8'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/e9/8d/31b50ac0879464049d744a1ddf00dc6474433eb55d40fa0c8e8510591ad2/pygls-1.3.0.tar.gz", hash = "sha256:1b44ace89c9382437a717534f490eadc6fda7c0c6c16ac1eaaf5568e345e4fb8", size = 45539 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/4e/1e/643070d8f5c851958662e7e5df16d9c3a068a598a7ee7bb2eb8d95b4e5d7/pygls-1.3.0-py3-none-any.whl", hash = "sha256:d4a01414b6ed4e34e7e8fd29b77d3e88c29615df7d0bbff49bf019e15ec04b8f", size = 56031 },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.7.1"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version < '3.8'",
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/3c/8b/0111dd7d6c1478bf83baa1cab85c686426c7a6274119aceb2bd9d35395ad/typing_extensions-4.7.1.tar.gz", hash = "sha256:b75ddc264f0ba5615db7ba217daeb99701ad295353c45f9e95963337ceeeffb2", size = 72876 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ec/6b/63cc3df74987c36fe26157ee12e09e8f9db4de771e0f3404263117e75b95/typing_extensions-4.7.1-py3-none-any.whl", hash = "sha256:440d5dd3af93b060174bf433bccd69b0babc3b15b1a8dca43789fd7f61514b36", size = 33232 },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.8'",
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 },
        ]

        [[package]]
        name = "zipp"
        version = "3.15.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/00/27/f0ac6b846684cecce1ee93d32450c45ab607f65c2e0255f0092032d91f07/zipp-3.15.0.tar.gz", hash = "sha256:112929ad649da941c23de50f356a2b5570c954b65150642bccdd66bf194d224b", size = 18454 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/5b/fa/c9e82bbe1af6266adf08afb563905eb87cab83fde00a0a08963510621047/zipp-3.15.0-py3-none-any.whl", hash = "sha256:48904fc76a60e542af151aded95726c1a5c34ed43ab4134b597665c86d7ad556", size = 6758 },
        ]
        "###
        );
    });

    // Remove the lockfile.
    fs_err::remove_file(&lockfile)?;

    // Bump the Python requirement even further.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["pygls"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(&lockfile).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "attrs"
        version = "23.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/e3/fc/f800d51204003fa8ae392c4e8278f256206e7a919b708eef054f5f4b650d/attrs-23.2.0.tar.gz", hash = "sha256:935dc3b529c262f6cf76e50877d35a4bd3c1de194fd41f47a2b7ae8f19971f30", size = 780820 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e0/44/827b2a91a5816512fcaf3cc4ebc465ccd5d598c45cefa6703fcf4a79018f/attrs-23.2.0-py3-none-any.whl", hash = "sha256:99b87a485a5820b23b879f04c2305b44b951b502fd64be915879d77a7e8fc6f1", size = 60752 },
        ]

        [[package]]
        name = "cattrs"
        version = "23.2.3"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "attrs" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/1e/57/c6ccd22658c4bcb3beb3f1c262e1f170cf136e913b122763d0ddd328d284/cattrs-23.2.3.tar.gz", hash = "sha256:a934090d95abaa9e911dac357e3a8699e0b4b14f8529bcc7d2b1ad9d51672b9f", size = 610215 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b3/0d/cd4a4071c7f38385dc5ba91286723b4d1090b87815db48216212c6c6c30e/cattrs-23.2.3-py3-none-any.whl", hash = "sha256:0341994d94971052e9ee70662542699a3162ea1e0c62f7ce1b4a57f563685108", size = 57474 },
        ]

        [[package]]
        name = "lsprotocol"
        version = "2023.0.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "attrs" },
            { name = "cattrs" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/9d/f6/6e80484ec078d0b50699ceb1833597b792a6c695f90c645fbaf54b947e6f/lsprotocol-2023.0.1.tar.gz", hash = "sha256:cc5c15130d2403c18b734304339e51242d3018a05c4f7d0f198ad6e0cd21861d", size = 69434 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/8d/37/2351e48cb3309673492d3a8c59d407b75fb6630e560eb27ecd4da03adc9a/lsprotocol-2023.0.1-py3-none-any.whl", hash = "sha256:c75223c9e4af2f24272b14c6375787438279369236cd568f596d4951052a60f2", size = 70826 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "pygls" },
        ]

        [package.metadata]
        requires-dist = [{ name = "pygls" }]

        [[package]]
        name = "pygls"
        version = "1.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "cattrs" },
            { name = "lsprotocol" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/e9/8d/31b50ac0879464049d744a1ddf00dc6474433eb55d40fa0c8e8510591ad2/pygls-1.3.0.tar.gz", hash = "sha256:1b44ace89c9382437a717534f490eadc6fda7c0c6c16ac1eaaf5568e345e4fb8", size = 45539 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/4e/1e/643070d8f5c851958662e7e5df16d9c3a068a598a7ee7bb2eb8d95b4e5d7/pygls-1.3.0-py3-none-any.whl", hash = "sha256:d4a01414b6ed4e34e7e8fd29b77d3e88c29615df7d0bbff49bf019e15ec04b8f", size = 56031 },
        ]
        "###
        );
    });

    // Validate that attempting to install with an unsupported Python version raises an error.
    let context38 = TestContext::new("3.8").with_filtered_python_sources();

    fs_err::copy(pyproject_toml, context38.temp_dir.join("pyproject.toml"))?;
    fs_err::copy(&lockfile, context38.temp_dir.join("uv.lock"))?;

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###);

    // Install from the lockfile.
    // Note we need to disable Python fetches or we'll just download 3.12
    uv_snapshot!(context38.filters(), context38.sync().arg("--frozen").arg("--no-python-downloads"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No interpreter found for Python >=3.12 in [PYTHON SOURCES]
    "###);

    Ok(())
}

/// Lock a requirement from PyPI, ignoring any dependencies that exceed the `requires-python`
/// upper-bound.
#[test]
fn lock_requires_python_upper() -> Result<()> {
    let context = TestContext::new("3.11");

    let lockfile = context.temp_dir.join("uv.lock");

    // Require `==3.11.*`.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "warehouse"
        version = "1.0.0"
        requires-python = "==3.11.*"
        dependencies = ["pydantic"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().env(EnvVars::UV_EXCLUDE_NEWER, "2024-08-29T00:00:00Z"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(&lockfile).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = "==3.11.*"

        [options]
        exclude-newer = "2024-08-29T00:00:00Z"

        [[package]]
        name = "annotated-types"
        version = "0.7.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/67/531ea369ba64dcff5ec9c3402f9f51bf748cec26dde048a2f973a4eea7f5/annotated_types-0.7.0.tar.gz", hash = "sha256:aff07c09a53a08bc8cfccb9c85b05f1aa9a2a6f23728d790723543408344ce89", size = 16081 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/78/b6/6307fbef88d9b5ee7421e68d78a9f162e0da4900bc5f5793f6d3d0e34fb8/annotated_types-0.7.0-py3-none-any.whl", hash = "sha256:1f02e8b43a8fbbc3f3e0d4f0f4bfc8131bcb4eebe8849b8e5c773f3a1c582a53", size = 13643 },
        ]

        [[package]]
        name = "pydantic"
        version = "2.8.2"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "annotated-types" },
            { name = "pydantic-core" },
            { name = "typing-extensions" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/8c/99/d0a5dca411e0a017762258013ba9905cd6e7baa9a3fd1fe8b6529472902e/pydantic-2.8.2.tar.gz", hash = "sha256:6f62c13d067b0755ad1c21a34bdd06c0c12625a22b0fc09c6b149816604f7c2a", size = 739834 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/1f/fa/b7f815b8c9ad021c07f88875b601222ef5e70619391ade4a49234d12d278/pydantic-2.8.2-py3-none-any.whl", hash = "sha256:73ee9fddd406dc318b885c7a2eab8a6472b68b8fb5ba8150949fc3db939f23c8", size = 423875 },
        ]

        [[package]]
        name = "pydantic-core"
        version = "2.20.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "typing-extensions" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/12/e3/0d5ad91211dba310f7ded335f4dad871172b9cc9ce204f5a56d76ccd6247/pydantic_core-2.20.1.tar.gz", hash = "sha256:26ca695eeee5f9f1aeeb211ffc12f10bcb6f71e2989988fda61dabd65db878d4", size = 388371 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/61/db/f6a724db226d990a329910727cfac43539ff6969edc217286dd05cda3ef6/pydantic_core-2.20.1-cp311-cp311-macosx_10_12_x86_64.whl", hash = "sha256:d2a8fa9d6d6f891f3deec72f5cc668e6f66b188ab14bb1ab52422fe8e644f312", size = 1834507 },
            { url = "https://files.pythonhosted.org/packages/9b/83/6f2bfe75209d557ae1c3550c1252684fc1827b8b12fbed84c3b4439e135d/pydantic_core-2.20.1-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:175873691124f3d0da55aeea1d90660a6ea7a3cfea137c38afa0a5ffabe37b88", size = 1773527 },
            { url = "https://files.pythonhosted.org/packages/93/ef/513ea76d7ca81f2354bb9c8d7839fc1157673e652613f7e1aff17d8ce05d/pydantic_core-2.20.1-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:37eee5b638f0e0dcd18d21f59b679686bbd18917b87db0193ae36f9c23c355fc", size = 1787879 },
            { url = "https://files.pythonhosted.org/packages/31/0a/ac294caecf235f0cc651de6232f1642bb793af448d1cfc541b0dc1fd72b8/pydantic_core-2.20.1-cp311-cp311-manylinux_2_17_armv7l.manylinux2014_armv7l.whl", hash = "sha256:25e9185e2d06c16ee438ed39bf62935ec436474a6ac4f9358524220f1b236e43", size = 1774694 },
            { url = "https://files.pythonhosted.org/packages/46/a4/08f12b5512f095963550a7cb49ae010e3f8f3f22b45e508c2cb4d7744fce/pydantic_core-2.20.1-cp311-cp311-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:150906b40ff188a3260cbee25380e7494ee85048584998c1e66df0c7a11c17a6", size = 1976369 },
            { url = "https://files.pythonhosted.org/packages/15/59/b2495be4410462aedb399071c71884042a2c6443319cbf62d00b4a7ed7a5/pydantic_core-2.20.1-cp311-cp311-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:8ad4aeb3e9a97286573c03df758fc7627aecdd02f1da04516a86dc159bf70121", size = 2691250 },
            { url = "https://files.pythonhosted.org/packages/3c/ae/fc99ce1ba791c9e9d1dee04ce80eef1dae5b25b27e3fc8e19f4e3f1348bf/pydantic_core-2.20.1-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:d3f3ed29cd9f978c604708511a1f9c2fdcb6c38b9aae36a51905b8811ee5cbf1", size = 2061462 },
            { url = "https://files.pythonhosted.org/packages/44/bb/eb07cbe47cfd638603ce3cb8c220f1a054b821e666509e535f27ba07ca5f/pydantic_core-2.20.1-cp311-cp311-manylinux_2_5_i686.manylinux1_i686.whl", hash = "sha256:b0dae11d8f5ded51699c74d9548dcc5938e0804cc8298ec0aa0da95c21fff57b", size = 1893923 },
            { url = "https://files.pythonhosted.org/packages/ce/ef/5a52400553b8faa0e7f11fd7a2ba11e8d2feb50b540f9e7973c49b97eac0/pydantic_core-2.20.1-cp311-cp311-musllinux_1_1_aarch64.whl", hash = "sha256:faa6b09ee09433b87992fb5a2859efd1c264ddc37280d2dd5db502126d0e7f27", size = 1966779 },
            { url = "https://files.pythonhosted.org/packages/4c/5b/fb37fe341344d9651f5c5f579639cd97d50a457dc53901aa8f7e9f28beb9/pydantic_core-2.20.1-cp311-cp311-musllinux_1_1_x86_64.whl", hash = "sha256:9dc1b507c12eb0481d071f3c1808f0529ad41dc415d0ca11f7ebfc666e66a18b", size = 2109044 },
            { url = "https://files.pythonhosted.org/packages/70/1a/6f7278802dbc66716661618807ab0dfa4fc32b09d1235923bbbe8b3a5757/pydantic_core-2.20.1-cp311-none-win32.whl", hash = "sha256:fa2fddcb7107e0d1808086ca306dcade7df60a13a6c347a7acf1ec139aa6789a", size = 1708265 },
            { url = "https://files.pythonhosted.org/packages/35/7f/58758c42c61b0bdd585158586fecea295523d49933cb33664ea888162daf/pydantic_core-2.20.1-cp311-none-win_amd64.whl", hash = "sha256:40a783fb7ee353c50bd3853e626f15677ea527ae556429453685ae32280c19c2", size = 1901750 },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.12.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/df/db/f35a00659bc03fec321ba8bce9420de607a1d37f8342eee1863174c69557/typing_extensions-4.12.2.tar.gz", hash = "sha256:1a7ead55c7e559dd4dee8856e3a88b41225abfe1ce8df57b7c13915fe121ffb8", size = 85321 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/26/9f/ad63fc0248c5379346306f8668cda6e2e2e9c95e01216d2b8ffd9ff037d0/typing_extensions-4.12.2-py3-none-any.whl", hash = "sha256:04e5ca0351e0f3f85c6853954072df659d0d13fac324d0072316b67d7794700d", size = 37438 },
        ]

        [[package]]
        name = "warehouse"
        version = "1.0.0"
        source = { editable = "." }
        dependencies = [
            { name = "pydantic" },
        ]

        [package.metadata]
        requires-dist = [{ name = "pydantic" }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").env(EnvVars::UV_EXCLUDE_NEWER, "2024-08-29T00:00:00Z"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###);

    Ok(())
}

/// Lock a requirement from PyPI with an exact Python bound.
#[cfg(feature = "python-patch")]
#[test]
fn lock_requires_python_exact() -> Result<()> {
    let context = TestContext::new("3.13.0");

    let lockfile = context.temp_dir.join("uv.lock");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "warehouse"
        version = "1.0.0"
        requires-python = "==3.13"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The workspace `requires-python` value (`==3.13`) contains an exact match without a patch version. When omitted, the patch version is implicitly `0` (e.g., `==3.13.0`). Did you mean `==3.13.*`?
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(&lockfile).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = "==3.13"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "warehouse"
        version = "1.0.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig" }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The workspace `requires-python` value (`==3.13`) contains an exact match without a patch version. When omitted, the patch version is implicitly `0` (e.g., `==3.13.0`). Did you mean `==3.13.*`?
    Resolved 2 packages in [TIME]
    "###);

    Ok(())
}

/// Fork, even with a single dependency, if the minimum Python version is increased.
#[test]
fn lock_requires_python_fork() -> Result<()> {
    let context = TestContext::new("3.11");

    let lockfile = context.temp_dir.join("uv.lock");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "warehouse"
        version = "1.0.0"
        requires-python = ">=3.9"
        dependencies = ["uv ; python_version>='3.8'"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().env(EnvVars::UV_EXCLUDE_NEWER, "2024-08-29T00:00:00Z"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(&lockfile).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.9"

        [options]
        exclude-newer = "2024-08-29T00:00:00Z"

        [[package]]
        name = "uv"
        version = "0.4.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/0f/dc/94b6609d89693be22119f8ff7f586f6125de6d6ff096daa06b5250760563/uv-0.4.0.tar.gz", hash = "sha256:1658a17b7c4c0ad750fc44a7ef1196e058fb0c18873f54420c17f3ce807bfc24", size = 1807995 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c6/1f/eeddd9565b2627495ee9588c2852e3c267f893247726aa4ee967df70d388/uv-0.4.0-py3-none-linux_armv6l.whl", hash = "sha256:3870d045d878e3da6505f4ebae7ecf01761ec481ae5de5e30e57e8e58557d755", size = 10760426 },
            { url = "https://files.pythonhosted.org/packages/76/be/0e5f3d36a5811315c5e97ac860ab80885865c79f64fcf8cf72a8e978f08f/uv-0.4.0-py3-none-macosx_10_12_x86_64.whl", hash = "sha256:de7171d3e3ea994c754e750b79c788735eea0e50a60f878225f23645f047dd5b", size = 11152293 },
            { url = "https://files.pythonhosted.org/packages/2e/86/9844e8ab08e25cbf2094e2fa1a7ad66563036bfed77b46986b6a2489c10c/uv-0.4.0-py3-none-macosx_11_0_arm64.whl", hash = "sha256:02e0295566454289348de502677e2240ad86f2cb5fa058504e1b2ca2a2ebf7e1", size = 10302231 },
            { url = "https://files.pythonhosted.org/packages/9d/75/28c3386d0649a5becc95b6fe323d7891548932aa93c8ed29c32b6ad3f52d/uv-0.4.0-py3-none-manylinux_2_17_aarch64.manylinux2014_aarch64.musllinux_1_1_aarch64.whl", hash = "sha256:6e0d1b257d87d46c1047f62fc32cddb46df510e7382dec232b4ebc2475cf0957", size = 10611491 },
            { url = "https://files.pythonhosted.org/packages/c9/50/8300c36a88cc1a62c261a84ebbdff0c69794825f26930d7a0ecee5d277b7/uv-0.4.0-py3-none-manylinux_2_17_armv7l.manylinux2014_armv7l.whl", hash = "sha256:787764145fb16f73eba04cce0855d18aeb0de3fc86e43aadd2ebe4992aa32c7f", size = 10579879 },
            { url = "https://files.pythonhosted.org/packages/a3/0f/eeb272ff4e86a39644e152a28fafa43798c1498b0ac39b2e2ae2c410d7be/uv-0.4.0-py3-none-manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:c83f59c3326f7169f927603cc4b766e321fa941f1047f70ce388292e08d0966b", size = 11168249 },
            { url = "https://files.pythonhosted.org/packages/b9/94/6655626f14585124fda2eb039781ff74a996da69437f197f74b7ac75f7a7/uv-0.4.0-py3-none-manylinux_2_17_ppc64.manylinux2014_ppc64.whl", hash = "sha256:0d6a1420b9ae8f391e733626ef583d1b328070e952ce83b65e4afb514ae03086", size = 11957706 },
            { url = "https://files.pythonhosted.org/packages/c1/b5/821a343e33233b4efcd3ce725d648f89cce2ffbaaa232cda1ab094ef0d82/uv-0.4.0-py3-none-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:7ef78d636b45e4b919d0bd3c17c2dc42600cdab2ad6e0dad971fb5a733398987", size = 11767024 },
            { url = "https://files.pythonhosted.org/packages/ce/4e/286122669389f87fb9fa57c5ccbc977b843ee69f54bdfc781520e6fe8a38/uv-0.4.0-py3-none-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:0671259b9a1ba67535382264ec8c000501d3bd9e3f9d28dbe6a57145a168fa31", size = 14705106 },
            { url = "https://files.pythonhosted.org/packages/0c/71/f7a8b9a0f49f2fa5c979bf7d10e83791043e3eec3e43d80099acee4368fd/uv-0.4.0-py3-none-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:85a94f21bebe4b3f452afe5fbc36ed326583c8b6cc7fe3091f5f571a727ed799", size = 11512897 },
            { url = "https://files.pythonhosted.org/packages/bf/91/b0b4cb51b5ec31ec45fa52eae2aaa91ad20196a66e94a3d593379195ca80/uv-0.4.0-py3-none-manylinux_2_28_aarch64.whl", hash = "sha256:d9eb82b0198f10cb5f0354d5c4483f6b304ac6f0b74131b88fbf092a2030268f", size = 10704876 },
            { url = "https://files.pythonhosted.org/packages/12/18/1055d2b5de7cc12fb83b2f3ba397869b8277d0a3c8524aa34fb96cc8f548/uv-0.4.0-py3-none-musllinux_1_1_armv7l.whl", hash = "sha256:90497e0413d76378000d8d62891d49e786065321ae9f68b933b0914f92cf3ea2", size = 10576751 },
            { url = "https://files.pythonhosted.org/packages/3d/05/637f37dc173635688e14f4f358d75b58d136a8d78c2fe60365fd56f7d5ae/uv-0.4.0-py3-none-musllinux_1_1_i686.whl", hash = "sha256:1e10d262f55857b4e85dc3b550cd4fa09714fa639d53886065f19aa1f09720f7", size = 11000442 },
            { url = "https://files.pythonhosted.org/packages/0a/21/bc467390c62493d6778c5c2c805375340b3d220073330011cd5e4562a161/uv-0.4.0-py3-none-musllinux_1_1_ppc64le.whl", hash = "sha256:80329b24feb52cf46187a50d6e163aad3afc52bc601218d876970d855bb71f9b", size = 12714226 },
            { url = "https://files.pythonhosted.org/packages/eb/12/9801ebf36cdd72baf695adef704999d830e824e78db7bf66491fc7712ca7/uv-0.4.0-py3-none-musllinux_1_1_x86_64.whl", hash = "sha256:e911e8e0c59144c54468fae5e4bcbd76f8bafc5c6ca33d14f88e2ce8a633f7ab", size = 11651071 },
            { url = "https://files.pythonhosted.org/packages/81/12/bd8bc40b3b88be4c75726b610302628edec6b2cbd4f58717403fb7cfc7c5/uv-0.4.0-py3-none-win32.whl", hash = "sha256:93861f0d0bf5c44ded97ee2b188f0c30e0521e7a51f3f63daf6e64766913f036", size = 10933271 },
            { url = "https://files.pythonhosted.org/packages/d6/c2/7219ee34993c50b3fedd81a7d77f27232df0bcf5451920dd16717fcc9b1f/uv-0.4.0-py3-none-win_amd64.whl", hash = "sha256:6b9b8db49928b71f926cf48150a34c69458447e554e7b65c1337d3e907bd7fb5", size = 12145169 },
        ]

        [[package]]
        name = "warehouse"
        version = "1.0.0"
        source = { editable = "." }
        dependencies = [
            { name = "uv" },
        ]

        [package.metadata]
        requires-dist = [{ name = "uv", marker = "python_full_version >= '3.8'" }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").env(EnvVars::UV_EXCLUDE_NEWER, "2024-08-29T00:00:00Z"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    Ok(())
}

/// Lock a requirement from PyPI, respecting the `Requires-Python` metadata
#[test]
fn lock_requires_python_wheels() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.11", "3.12"]);

    let lockfile = context.temp_dir.join("uv.lock");

    // Require ==3.12.*.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = "==3.12.*"
        dependencies = ["frozenlist"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(&lockfile).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = "==3.12.*"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "frozenlist"
        version = "1.4.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/cf/3d/2102257e7acad73efc4a0c306ad3953f68c504c16982bbdfee3ad75d8085/frozenlist-1.4.1.tar.gz", hash = "sha256:c037a86e8513059a2613aaba4d817bb90b9d9b6b69aace3ce9c877e8c8ed402b", size = 37820 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b4/db/4cf37556a735bcdb2582f2c3fa286aefde2322f92d3141e087b8aeb27177/frozenlist-1.4.1-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:1979bc0aeb89b33b588c51c54ab0161791149f2461ea7c7c946d95d5f93b56ae", size = 93937 },
            { url = "https://files.pythonhosted.org/packages/46/03/69eb64642ca8c05f30aa5931d6c55e50b43d0cd13256fdd01510a1f85221/frozenlist-1.4.1-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:cc7b01b3754ea68a62bd77ce6020afaffb44a590c2289089289363472d13aedb", size = 53656 },
            { url = "https://files.pythonhosted.org/packages/3f/ab/c543c13824a615955f57e082c8a5ee122d2d5368e80084f2834e6f4feced/frozenlist-1.4.1-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:c9c92be9fd329ac801cc420e08452b70e7aeab94ea4233a4804f0915c14eba9b", size = 51868 },
            { url = "https://files.pythonhosted.org/packages/a9/b8/438cfd92be2a124da8259b13409224d9b19ef8f5a5b2507174fc7e7ea18f/frozenlist-1.4.1-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:5c3894db91f5a489fc8fa6a9991820f368f0b3cbdb9cd8849547ccfab3392d86", size = 280652 },
            { url = "https://files.pythonhosted.org/packages/54/72/716a955521b97a25d48315c6c3653f981041ce7a17ff79f701298195bca3/frozenlist-1.4.1-cp312-cp312-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:ba60bb19387e13597fb059f32cd4d59445d7b18b69a745b8f8e5db0346f33480", size = 286739 },
            { url = "https://files.pythonhosted.org/packages/65/d8/934c08103637567084568e4d5b4219c1016c60b4d29353b1a5b3587827d6/frozenlist-1.4.1-cp312-cp312-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:8aefbba5f69d42246543407ed2461db31006b0f76c4e32dfd6f42215a2c41d09", size = 289447 },
            { url = "https://files.pythonhosted.org/packages/70/bb/d3b98d83ec6ef88f9bd63d77104a305d68a146fd63a683569ea44c3085f6/frozenlist-1.4.1-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:780d3a35680ced9ce682fbcf4cb9c2bad3136eeff760ab33707b71db84664e3a", size = 265466 },
            { url = "https://files.pythonhosted.org/packages/0b/f2/b8158a0f06faefec33f4dff6345a575c18095a44e52d4f10c678c137d0e0/frozenlist-1.4.1-cp312-cp312-manylinux_2_5_x86_64.manylinux1_x86_64.manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:9acbb16f06fe7f52f441bb6f413ebae6c37baa6ef9edd49cdd567216da8600cd", size = 281530 },
            { url = "https://files.pythonhosted.org/packages/ea/a2/20882c251e61be653764038ece62029bfb34bd5b842724fff32a5b7a2894/frozenlist-1.4.1-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:23b701e65c7b36e4bf15546a89279bd4d8675faabc287d06bbcfac7d3c33e1e6", size = 281295 },
            { url = "https://files.pythonhosted.org/packages/4c/f9/8894c05dc927af2a09663bdf31914d4fb5501653f240a5bbaf1e88cab1d3/frozenlist-1.4.1-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:3e0153a805a98f5ada7e09826255ba99fb4f7524bb81bf6b47fb702666484ae1", size = 268054 },
            { url = "https://files.pythonhosted.org/packages/37/ff/a613e58452b60166507d731812f3be253eb1229808e59980f0405d1eafbf/frozenlist-1.4.1-cp312-cp312-musllinux_1_1_ppc64le.whl", hash = "sha256:dd9b1baec094d91bf36ec729445f7769d0d0cf6b64d04d86e45baf89e2b9059b", size = 286904 },
            { url = "https://files.pythonhosted.org/packages/cc/6e/0091d785187f4c2020d5245796d04213f2261ad097e0c1cf35c44317d517/frozenlist-1.4.1-cp312-cp312-musllinux_1_1_s390x.whl", hash = "sha256:1a4471094e146b6790f61b98616ab8e44f72661879cc63fa1049d13ef711e71e", size = 290754 },
            { url = "https://files.pythonhosted.org/packages/a5/c2/e42ad54bae8bcffee22d1e12a8ee6c7717f7d5b5019261a8c861854f4776/frozenlist-1.4.1-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:5667ed53d68d91920defdf4035d1cdaa3c3121dc0b113255124bcfada1cfa1b8", size = 282602 },
            { url = "https://files.pythonhosted.org/packages/b6/61/56bad8cb94f0357c4bc134acc30822e90e203b5cb8ff82179947de90c17f/frozenlist-1.4.1-cp312-cp312-win32.whl", hash = "sha256:beee944ae828747fd7cb216a70f120767fc9f4f00bacae8543c14a6831673f89", size = 44063 },
            { url = "https://files.pythonhosted.org/packages/3e/dc/96647994a013bc72f3d453abab18340b7f5e222b7b7291e3697ca1fcfbd5/frozenlist-1.4.1-cp312-cp312-win_amd64.whl", hash = "sha256:64536573d0a2cb6e625cf309984e2d873979709f2cf22839bf2d61790b448ad5", size = 50452 },
            { url = "https://files.pythonhosted.org/packages/83/10/466fe96dae1bff622021ee687f68e5524d6392b0a2f80d05001cd3a451ba/frozenlist-1.4.1-py3-none-any.whl", hash = "sha256:04ced3e6a46b4cfffe20f9ae482818e34eba9b5fb0ce4056e4cc9b6e212d09b7", size = 11552 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "frozenlist" },
        ]

        [package.metadata]
        requires-dist = [{ name = "frozenlist" }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 2 packages in [TIME]
    "###);

    // Change to ==3.11.*, which should different wheels in the lockfile.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = "==3.11.*"
        dependencies = ["frozenlist"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.11.[X] interpreter at: [PYTHON-3.11]
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(&lockfile).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = "==3.11.*"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "frozenlist"
        version = "1.4.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/cf/3d/2102257e7acad73efc4a0c306ad3953f68c504c16982bbdfee3ad75d8085/frozenlist-1.4.1.tar.gz", hash = "sha256:c037a86e8513059a2613aaba4d817bb90b9d9b6b69aace3ce9c877e8c8ed402b", size = 37820 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/01/bc/8d33f2d84b9368da83e69e42720cff01c5e199b5a868ba4486189a4d8fa9/frozenlist-1.4.1-cp311-cp311-macosx_10_9_universal2.whl", hash = "sha256:a0cb6f11204443f27a1628b0e460f37fb30f624be6051d490fa7d7e26d4af3d0", size = 97060 },
            { url = "https://files.pythonhosted.org/packages/af/b2/904500d6a162b98a70e510e743e7ea992241b4f9add2c8063bf666ca21df/frozenlist-1.4.1-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:b46c8ae3a8f1f41a0d2ef350c0b6e65822d80772fe46b653ab6b6274f61d4a49", size = 55347 },
            { url = "https://files.pythonhosted.org/packages/5b/9c/f12b69997d3891ddc0d7895999a00b0c6a67f66f79498c0e30f27876435d/frozenlist-1.4.1-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:fde5bd59ab5357e3853313127f4d3565fc7dad314a74d7b5d43c22c6a5ed2ced", size = 53374 },
            { url = "https://files.pythonhosted.org/packages/ac/6e/e0322317b7c600ba21dec224498c0c5959b2bce3865277a7c0badae340a9/frozenlist-1.4.1-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:722e1124aec435320ae01ee3ac7bec11a5d47f25d0ed6328f2273d287bc3abb0", size = 273288 },
            { url = "https://files.pythonhosted.org/packages/a7/76/180ee1b021568dad5b35b7678616c24519af130ed3fa1e0f1ed4014e0f93/frozenlist-1.4.1-cp311-cp311-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:2471c201b70d58a0f0c1f91261542a03d9a5e088ed3dc6c160d614c01649c106", size = 284737 },
            { url = "https://files.pythonhosted.org/packages/05/08/40159d706a6ed983c8aca51922a93fc69f3c27909e82c537dd4054032674/frozenlist-1.4.1-cp311-cp311-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:c757a9dd70d72b076d6f68efdbb9bc943665ae954dad2801b874c8c69e185068", size = 280267 },
            { url = "https://files.pythonhosted.org/packages/e0/18/9f09f84934c2b2aa37d539a322267939770362d5495f37783440ca9c1b74/frozenlist-1.4.1-cp311-cp311-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:f146e0911cb2f1da549fc58fc7bcd2b836a44b79ef871980d605ec392ff6b0d2", size = 258778 },
            { url = "https://files.pythonhosted.org/packages/b3/c9/0bc5ee7e1f5cc7358ab67da0b7dfe60fbd05c254cea5c6108e7d1ae28c63/frozenlist-1.4.1-cp311-cp311-manylinux_2_5_x86_64.manylinux1_x86_64.manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:4f9c515e7914626b2a2e1e311794b4c35720a0be87af52b79ff8e1429fc25f19", size = 272276 },
            { url = "https://files.pythonhosted.org/packages/12/5d/147556b73a53ad4df6da8bbb50715a66ac75c491fdedac3eca8b0b915345/frozenlist-1.4.1-cp311-cp311-musllinux_1_1_aarch64.whl", hash = "sha256:c302220494f5c1ebeb0912ea782bcd5e2f8308037b3c7553fad0e48ebad6ad82", size = 272424 },
            { url = "https://files.pythonhosted.org/packages/83/61/2087bbf24070b66090c0af922685f1d0596c24bb3f3b5223625bdeaf03ca/frozenlist-1.4.1-cp311-cp311-musllinux_1_1_i686.whl", hash = "sha256:442acde1e068288a4ba7acfe05f5f343e19fac87bfc96d89eb886b0363e977ec", size = 260881 },
            { url = "https://files.pythonhosted.org/packages/a8/be/a235bc937dd803258a370fe21b5aa2dd3e7bfe0287a186a4bec30c6cccd6/frozenlist-1.4.1-cp311-cp311-musllinux_1_1_ppc64le.whl", hash = "sha256:1b280e6507ea8a4fa0c0a7150b4e526a8d113989e28eaaef946cc77ffd7efc0a", size = 282327 },
            { url = "https://files.pythonhosted.org/packages/5d/e7/b2469e71f082948066b9382c7b908c22552cc705b960363c390d2e23f587/frozenlist-1.4.1-cp311-cp311-musllinux_1_1_s390x.whl", hash = "sha256:fe1a06da377e3a1062ae5fe0926e12b84eceb8a50b350ddca72dc85015873f74", size = 281502 },
            { url = "https://files.pythonhosted.org/packages/db/1b/6a5b970e55dffc1a7d0bb54f57b184b2a2a2ad0b7bca16a97ca26d73c5b5/frozenlist-1.4.1-cp311-cp311-musllinux_1_1_x86_64.whl", hash = "sha256:db9e724bebd621d9beca794f2a4ff1d26eed5965b004a97f1f1685a173b869c2", size = 272292 },
            { url = "https://files.pythonhosted.org/packages/1a/05/ebad68130e6b6eb9b287dacad08ea357c33849c74550c015b355b75cc714/frozenlist-1.4.1-cp311-cp311-win32.whl", hash = "sha256:e774d53b1a477a67838a904131c4b0eef6b3d8a651f8b138b04f748fccfefe17", size = 44446 },
            { url = "https://files.pythonhosted.org/packages/b3/21/c5aaffac47fd305d69df46cfbf118768cdf049a92ee6b0b5cb029d449dcf/frozenlist-1.4.1-cp311-cp311-win_amd64.whl", hash = "sha256:fb3c2db03683b5767dedb5769b8a40ebb47d6f7f45b1b3e3b4b51ec8ad9d9825", size = 50459 },
            { url = "https://files.pythonhosted.org/packages/83/10/466fe96dae1bff622021ee687f68e5524d6392b0a2f80d05001cd3a451ba/frozenlist-1.4.1-py3-none-any.whl", hash = "sha256:04ced3e6a46b4cfffe20f9ae482818e34eba9b5fb0ce4056e4cc9b6e212d09b7", size = 11552 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "frozenlist" },
        ]

        [package.metadata]
        requires-dist = [{ name = "frozenlist" }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.11.[X] interpreter at: [PYTHON-3.11]
    Resolved 2 packages in [TIME]
    "###);

    Ok(())
}

/// Lock a requirement from PyPI, respecting the `Requires-Python` metadata. In this case,
/// `Requires-Python` uses the equals-star syntax.
#[test]
fn lock_requires_python_star() -> Result<()> {
    let context = TestContext::new("3.11");

    let lockfile = context.temp_dir.join("uv.lock");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = "==3.11.*"
        dependencies = ["linehaul"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(&lockfile).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = "==3.11.*"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "attrs"
        version = "23.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/e3/fc/f800d51204003fa8ae392c4e8278f256206e7a919b708eef054f5f4b650d/attrs-23.2.0.tar.gz", hash = "sha256:935dc3b529c262f6cf76e50877d35a4bd3c1de194fd41f47a2b7ae8f19971f30", size = 780820 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e0/44/827b2a91a5816512fcaf3cc4ebc465ccd5d598c45cefa6703fcf4a79018f/attrs-23.2.0-py3-none-any.whl", hash = "sha256:99b87a485a5820b23b879f04c2305b44b951b502fd64be915879d77a7e8fc6f1", size = 60752 },
        ]

        [[package]]
        name = "cattrs"
        version = "23.2.3"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "attrs" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/1e/57/c6ccd22658c4bcb3beb3f1c262e1f170cf136e913b122763d0ddd328d284/cattrs-23.2.3.tar.gz", hash = "sha256:a934090d95abaa9e911dac357e3a8699e0b4b14f8529bcc7d2b1ad9d51672b9f", size = 610215 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b3/0d/cd4a4071c7f38385dc5ba91286723b4d1090b87815db48216212c6c6c30e/cattrs-23.2.3-py3-none-any.whl", hash = "sha256:0341994d94971052e9ee70662542699a3162ea1e0c62f7ce1b4a57f563685108", size = 57474 },
        ]

        [[package]]
        name = "linehaul"
        version = "1.0.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "cattrs" },
            { name = "packaging" },
            { name = "pyparsing" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/f8/e7/74d1bd36ed26ac43bfe22e97129edaa7066f7af4bf76084b9493cd581d58/linehaul-1.0.1.tar.gz", hash = "sha256:09d71b1f6a9ab92dd8c763b3d099e4ae05c2845ee783a02d5fe731e6e2a6a997", size = 19410 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/03/73/c73588052198be06462d1a7c4653b602a109a0df0208c59e58075dc3bc73/linehaul-1.0.1-py3-none-any.whl", hash = "sha256:d19ca669008dad910868dfae7f904dfc5362583729bda344799cf7ea2ad5ef12", size = 27848 },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "linehaul" },
        ]

        [package.metadata]
        requires-dist = [{ name = "linehaul" }]

        [[package]]
        name = "pyparsing"
        version = "3.1.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/46/3a/31fd28064d016a2182584d579e033ec95b809d8e220e74c4af6f0f2e8842/pyparsing-3.1.2.tar.gz", hash = "sha256:a1bac0ce561155ecc3ed78ca94d3c9378656ad4c94c1270de543f621420f94ad", size = 889571 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/9d/ea/6d76df31432a0e6fdf81681a895f009a4bb47b3c39036db3e1b528191d52/pyparsing-3.1.2-py3-none-any.whl", hash = "sha256:f9db75911801ed778fe61bb643079ff86601aca99fcae6345aa67292038fb742", size = 103245 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "###);

    Ok(())
}

/// Lock a requirement from PyPI, respecting the `Requires-Python` metadata. In this case,
/// `Requires-Python` uses the != operator.
#[test]
fn lock_requires_python_not_equal() -> Result<()> {
    let context = TestContext::new("3.12");

    let lockfile = context.temp_dir.join("uv.lock");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">3.10, !=3.10.9, !=3.10.10, !=3.11.*, <3.13"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(&lockfile).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">3.10, !=3.10.9, !=3.10.10, !=3.11.*, <3.13"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig" }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    Ok(())
}

/// Lock a requirement from PyPI, respecting the `Requires-Python` metadata. In this case,
/// `Requires-Python` uses a pre-release specifier, but it's effectively ignored, as `>=3.11.0b1`
/// is interpreted as equivalent to `>=3.11.0`.
#[test]
fn lock_requires_python_pre() -> Result<()> {
    let context = TestContext::new("3.11");

    let lockfile = context.temp_dir.join("uv.lock");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.11b1"
        dependencies = ["linehaul"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(&lockfile).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.11"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "attrs"
        version = "23.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/e3/fc/f800d51204003fa8ae392c4e8278f256206e7a919b708eef054f5f4b650d/attrs-23.2.0.tar.gz", hash = "sha256:935dc3b529c262f6cf76e50877d35a4bd3c1de194fd41f47a2b7ae8f19971f30", size = 780820 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e0/44/827b2a91a5816512fcaf3cc4ebc465ccd5d598c45cefa6703fcf4a79018f/attrs-23.2.0-py3-none-any.whl", hash = "sha256:99b87a485a5820b23b879f04c2305b44b951b502fd64be915879d77a7e8fc6f1", size = 60752 },
        ]

        [[package]]
        name = "cattrs"
        version = "23.2.3"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "attrs" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/1e/57/c6ccd22658c4bcb3beb3f1c262e1f170cf136e913b122763d0ddd328d284/cattrs-23.2.3.tar.gz", hash = "sha256:a934090d95abaa9e911dac357e3a8699e0b4b14f8529bcc7d2b1ad9d51672b9f", size = 610215 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b3/0d/cd4a4071c7f38385dc5ba91286723b4d1090b87815db48216212c6c6c30e/cattrs-23.2.3-py3-none-any.whl", hash = "sha256:0341994d94971052e9ee70662542699a3162ea1e0c62f7ce1b4a57f563685108", size = 57474 },
        ]

        [[package]]
        name = "linehaul"
        version = "1.0.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "cattrs" },
            { name = "packaging" },
            { name = "pyparsing" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/f8/e7/74d1bd36ed26ac43bfe22e97129edaa7066f7af4bf76084b9493cd581d58/linehaul-1.0.1.tar.gz", hash = "sha256:09d71b1f6a9ab92dd8c763b3d099e4ae05c2845ee783a02d5fe731e6e2a6a997", size = 19410 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/03/73/c73588052198be06462d1a7c4653b602a109a0df0208c59e58075dc3bc73/linehaul-1.0.1-py3-none-any.whl", hash = "sha256:d19ca669008dad910868dfae7f904dfc5362583729bda344799cf7ea2ad5ef12", size = 27848 },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "linehaul" },
        ]

        [package.metadata]
        requires-dist = [{ name = "linehaul" }]

        [[package]]
        name = "pyparsing"
        version = "3.1.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/46/3a/31fd28064d016a2182584d579e033ec95b809d8e220e74c4af6f0f2e8842/pyparsing-3.1.2.tar.gz", hash = "sha256:a1bac0ce561155ecc3ed78ca94d3c9378656ad4c94c1270de543f621420f94ad", size = 889571 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/9d/ea/6d76df31432a0e6fdf81681a895f009a4bb47b3c39036db3e1b528191d52/pyparsing-3.1.2-py3-none-any.whl", hash = "sha256:f9db75911801ed778fe61bb643079ff86601aca99fcae6345aa67292038fb742", size = 103245 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "###);

    Ok(())
}

/// Warn if `Requires-Python` does not include a lower bound.
#[test]
fn lock_requires_python_unbounded() -> Result<()> {
    let context = TestContext::new("3.11");

    let lockfile = context.temp_dir.join("uv.lock");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = "<=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The workspace `requires-python` value (`<=3.12`) does not contain a lower bound. Add a lower bound to indicate the minimum compatible Python version (e.g., `>=3.11`).
    Resolved 3 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(&lockfile).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = "<=3.12"
        resolution-markers = [
            "python_full_version >= '3.7'",
            "python_full_version < '3.7'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "1.1.1"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version < '3.7'",
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/23/a2/97899f6bd0e873fed3a7e67ae8d3a08b21799430fb4da15cfedf10d6e2c2/iniconfig-1.1.1.tar.gz", hash = "sha256:bc3af051d7d14b2ee5ef9969666def0cd1a000e121eaea580d4a313df4b37f32", size = 8104 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl", hash = "sha256:011e24c64b7f47f6ebd835bb12a743f2fbe9a26d4cecaa7f53bc4f35ee9da8b3", size = 4990 },
        ]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.7'",
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig", version = "1.1.1", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version < '3.7'" },
            { name = "iniconfig", version = "2.0.0", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version >= '3.7'" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig" }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The workspace `requires-python` value (`<=3.12`) does not contain a lower bound. Add a lower bound to indicate the minimum compatible Python version (e.g., `>=3.11`).
    Resolved 3 packages in [TIME]
    "###);

    Ok(())
}

#[test]
fn lock_requires_python_maximum_version() -> Result<()> {
    let context = TestContext::new("3.11");

    let lockfile = context.temp_dir.join("uv.lock");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.8"
        dependencies = ["numpy"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(&lockfile).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.8"
        resolution-markers = [
            "python_full_version >= '3.9'",
            "python_full_version < '3.9'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "numpy"
        version = "1.24.4"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version < '3.9'",
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/a4/9b/027bec52c633f6556dba6b722d9a0befb40498b9ceddd29cbe67a45a127c/numpy-1.24.4.tar.gz", hash = "sha256:80f5e3a4e498641401868df4208b74581206afbee7cf7b8329daae82676d9463", size = 10911229 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/6b/80/6cdfb3e275d95155a34659163b83c09e3a3ff9f1456880bec6cc63d71083/numpy-1.24.4-cp310-cp310-macosx_10_9_x86_64.whl", hash = "sha256:c0bfb52d2169d58c1cdb8cc1f16989101639b34c7d3ce60ed70b19c63eba0b64", size = 19789140 },
            { url = "https://files.pythonhosted.org/packages/64/5f/3f01d753e2175cfade1013eea08db99ba1ee4bdb147ebcf3623b75d12aa7/numpy-1.24.4-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:ed094d4f0c177b1b8e7aa9cba7d6ceed51c0e569a5318ac0ca9a090680a6a1b1", size = 13854297 },
            { url = "https://files.pythonhosted.org/packages/5a/b3/2f9c21d799fa07053ffa151faccdceeb69beec5a010576b8991f614021f7/numpy-1.24.4-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:79fc682a374c4a8ed08b331bef9c5f582585d1048fa6d80bc6c35bc384eee9b4", size = 13995611 },
            { url = "https://files.pythonhosted.org/packages/10/be/ae5bf4737cb79ba437879915791f6f26d92583c738d7d960ad94e5c36adf/numpy-1.24.4-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:7ffe43c74893dbf38c2b0a1f5428760a1a9c98285553c89e12d70a96a7f3a4d6", size = 17282357 },
            { url = "https://files.pythonhosted.org/packages/c0/64/908c1087be6285f40e4b3e79454552a701664a079321cff519d8c7051d06/numpy-1.24.4-cp310-cp310-win32.whl", hash = "sha256:4c21decb6ea94057331e111a5bed9a79d335658c27ce2adb580fb4d54f2ad9bc", size = 12429222 },
            { url = "https://files.pythonhosted.org/packages/22/55/3d5a7c1142e0d9329ad27cece17933b0e2ab4e54ddc5c1861fbfeb3f7693/numpy-1.24.4-cp310-cp310-win_amd64.whl", hash = "sha256:b4bea75e47d9586d31e892a7401f76e909712a0fd510f58f5337bea9572c571e", size = 14841514 },
            { url = "https://files.pythonhosted.org/packages/a9/cc/5ed2280a27e5dab12994c884f1f4d8c3bd4d885d02ae9e52a9d213a6a5e2/numpy-1.24.4-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:f136bab9c2cfd8da131132c2cf6cc27331dd6fae65f95f69dcd4ae3c3639c810", size = 19775508 },
            { url = "https://files.pythonhosted.org/packages/c0/bc/77635c657a3668cf652806210b8662e1aff84b818a55ba88257abf6637a8/numpy-1.24.4-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:e2926dac25b313635e4d6cf4dc4e51c8c0ebfed60b801c799ffc4c32bf3d1254", size = 13840033 },
            { url = "https://files.pythonhosted.org/packages/a7/4c/96cdaa34f54c05e97c1c50f39f98d608f96f0677a6589e64e53104e22904/numpy-1.24.4-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:222e40d0e2548690405b0b3c7b21d1169117391c2e82c378467ef9ab4c8f0da7", size = 13991951 },
            { url = "https://files.pythonhosted.org/packages/22/97/dfb1a31bb46686f09e68ea6ac5c63fdee0d22d7b23b8f3f7ea07712869ef/numpy-1.24.4-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:7215847ce88a85ce39baf9e89070cb860c98fdddacbaa6c0da3ffb31b3350bd5", size = 17278923 },
            { url = "https://files.pythonhosted.org/packages/35/e2/76a11e54139654a324d107da1d98f99e7aa2a7ef97cfd7c631fba7dbde71/numpy-1.24.4-cp311-cp311-win32.whl", hash = "sha256:4979217d7de511a8d57f4b4b5b2b965f707768440c17cb70fbf254c4b225238d", size = 12422446 },
            { url = "https://files.pythonhosted.org/packages/d8/ec/ebef2f7d7c28503f958f0f8b992e7ce606fb74f9e891199329d5f5f87404/numpy-1.24.4-cp311-cp311-win_amd64.whl", hash = "sha256:b7b1fc9864d7d39e28f41d089bfd6353cb5f27ecd9905348c24187a768c79694", size = 14834466 },
            { url = "https://files.pythonhosted.org/packages/11/10/943cfb579f1a02909ff96464c69893b1d25be3731b5d3652c2e0cf1281ea/numpy-1.24.4-cp38-cp38-macosx_10_9_x86_64.whl", hash = "sha256:1452241c290f3e2a312c137a9999cdbf63f78864d63c79039bda65ee86943f61", size = 19780722 },
            { url = "https://files.pythonhosted.org/packages/a7/ae/f53b7b265fdc701e663fbb322a8e9d4b14d9cb7b2385f45ddfabfc4327e4/numpy-1.24.4-cp38-cp38-macosx_11_0_arm64.whl", hash = "sha256:04640dab83f7c6c85abf9cd729c5b65f1ebd0ccf9de90b270cd61935eef0197f", size = 13843102 },
            { url = "https://files.pythonhosted.org/packages/25/6f/2586a50ad72e8dbb1d8381f837008a0321a3516dfd7cb57fc8cf7e4bb06b/numpy-1.24.4-cp38-cp38-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:a5425b114831d1e77e4b5d812b69d11d962e104095a5b9c3b641a218abcc050e", size = 14039616 },
            { url = "https://files.pythonhosted.org/packages/98/5d/5738903efe0ecb73e51eb44feafba32bdba2081263d40c5043568ff60faf/numpy-1.24.4-cp38-cp38-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:dd80e219fd4c71fc3699fc1dadac5dcf4fd882bfc6f7ec53d30fa197b8ee22dc", size = 17316263 },
            { url = "https://files.pythonhosted.org/packages/d1/57/8d328f0b91c733aa9aa7ee540dbc49b58796c862b4fbcb1146c701e888da/numpy-1.24.4-cp38-cp38-win32.whl", hash = "sha256:4602244f345453db537be5314d3983dbf5834a9701b7723ec28923e2889e0bb2", size = 12455660 },
            { url = "https://files.pythonhosted.org/packages/69/65/0d47953afa0ad569d12de5f65d964321c208492064c38fe3b0b9744f8d44/numpy-1.24.4-cp38-cp38-win_amd64.whl", hash = "sha256:692f2e0f55794943c5bfff12b3f56f99af76f902fc47487bdfe97856de51a706", size = 14868112 },
            { url = "https://files.pythonhosted.org/packages/9a/cd/d5b0402b801c8a8b56b04c1e85c6165efab298d2f0ab741c2406516ede3a/numpy-1.24.4-cp39-cp39-macosx_10_9_x86_64.whl", hash = "sha256:2541312fbf09977f3b3ad449c4e5f4bb55d0dbf79226d7724211acc905049400", size = 19816549 },
            { url = "https://files.pythonhosted.org/packages/14/27/638aaa446f39113a3ed38b37a66243e21b38110d021bfcb940c383e120f2/numpy-1.24.4-cp39-cp39-macosx_11_0_arm64.whl", hash = "sha256:9667575fb6d13c95f1b36aca12c5ee3356bf001b714fc354eb5465ce1609e62f", size = 13879950 },
            { url = "https://files.pythonhosted.org/packages/8f/27/91894916e50627476cff1a4e4363ab6179d01077d71b9afed41d9e1f18bf/numpy-1.24.4-cp39-cp39-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:f3a86ed21e4f87050382c7bc96571755193c4c1392490744ac73d660e8f564a9", size = 14030228 },
            { url = "https://files.pythonhosted.org/packages/7a/7c/d7b2a0417af6428440c0ad7cb9799073e507b1a465f827d058b826236964/numpy-1.24.4-cp39-cp39-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:d11efb4dbecbdf22508d55e48d9c8384db795e1b7b51ea735289ff96613ff74d", size = 17311170 },
            { url = "https://files.pythonhosted.org/packages/18/9d/e02ace5d7dfccee796c37b995c63322674daf88ae2f4a4724c5dd0afcc91/numpy-1.24.4-cp39-cp39-win32.whl", hash = "sha256:6620c0acd41dbcb368610bb2f4d83145674040025e5536954782467100aa8835", size = 12454918 },
            { url = "https://files.pythonhosted.org/packages/63/38/6cc19d6b8bfa1d1a459daf2b3fe325453153ca7019976274b6f33d8b5663/numpy-1.24.4-cp39-cp39-win_amd64.whl", hash = "sha256:befe2bf740fd8373cf56149a5c23a0f601e82869598d41f8e188a0e9869926f8", size = 14867441 },
            { url = "https://files.pythonhosted.org/packages/a4/fd/8dff40e25e937c94257455c237b9b6bf5a30d42dd1cc11555533be099492/numpy-1.24.4-pp38-pypy38_pp73-macosx_10_9_x86_64.whl", hash = "sha256:31f13e25b4e304632a4619d0e0777662c2ffea99fcae2029556b17d8ff958aef", size = 19156590 },
            { url = "https://files.pythonhosted.org/packages/42/e7/4bf953c6e05df90c6d351af69966384fed8e988d0e8c54dad7103b59f3ba/numpy-1.24.4-pp38-pypy38_pp73-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:95f7ac6540e95bc440ad77f56e520da5bf877f87dca58bd095288dce8940532a", size = 16705744 },
            { url = "https://files.pythonhosted.org/packages/fc/dd/9106005eb477d022b60b3817ed5937a43dad8fd1f20b0610ea8a32fcb407/numpy-1.24.4-pp38-pypy38_pp73-win_amd64.whl", hash = "sha256:e98f220aa76ca2a977fe435f5b04d7b3470c0a2e6312907b37ba6068f26787f2", size = 14734290 },
        ]

        [[package]]
        name = "numpy"
        version = "1.26.4"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.9'",
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/65/6e/09db70a523a96d25e115e71cc56a6f9031e7b8cd166c1ac8438307c14058/numpy-1.26.4.tar.gz", hash = "sha256:2a02aba9ed12e4ac4eb3ea9421c420301a0c6460d9830d74a9df87efa4912010", size = 15786129 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a7/94/ace0fdea5241a27d13543ee117cbc65868e82213fb31a8eb7fe9ff23f313/numpy-1.26.4-cp310-cp310-macosx_10_9_x86_64.whl", hash = "sha256:9ff0f4f29c51e2803569d7a51c2304de5554655a60c5d776e35b4a41413830d0", size = 20631468 },
            { url = "https://files.pythonhosted.org/packages/20/f7/b24208eba89f9d1b58c1668bc6c8c4fd472b20c45573cb767f59d49fb0f6/numpy-1.26.4-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:2e4ee3380d6de9c9ec04745830fd9e2eccb3e6cf790d39d7b98ffd19b0dd754a", size = 13966411 },
            { url = "https://files.pythonhosted.org/packages/fc/a5/4beee6488160798683eed5bdb7eead455892c3b4e1f78d79d8d3f3b084ac/numpy-1.26.4-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:d209d8969599b27ad20994c8e41936ee0964e6da07478d6c35016bc386b66ad4", size = 14219016 },
            { url = "https://files.pythonhosted.org/packages/4b/d7/ecf66c1cd12dc28b4040b15ab4d17b773b87fa9d29ca16125de01adb36cd/numpy-1.26.4-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:ffa75af20b44f8dba823498024771d5ac50620e6915abac414251bd971b4529f", size = 18240889 },
            { url = "https://files.pythonhosted.org/packages/24/03/6f229fe3187546435c4f6f89f6d26c129d4f5bed40552899fcf1f0bf9e50/numpy-1.26.4-cp310-cp310-musllinux_1_1_aarch64.whl", hash = "sha256:62b8e4b1e28009ef2846b4c7852046736bab361f7aeadeb6a5b89ebec3c7055a", size = 13876746 },
            { url = "https://files.pythonhosted.org/packages/39/fe/39ada9b094f01f5a35486577c848fe274e374bbf8d8f472e1423a0bbd26d/numpy-1.26.4-cp310-cp310-musllinux_1_1_x86_64.whl", hash = "sha256:a4abb4f9001ad2858e7ac189089c42178fcce737e4169dc61321660f1a96c7d2", size = 18078620 },
            { url = "https://files.pythonhosted.org/packages/d5/ef/6ad11d51197aad206a9ad2286dc1aac6a378059e06e8cf22cd08ed4f20dc/numpy-1.26.4-cp310-cp310-win32.whl", hash = "sha256:bfe25acf8b437eb2a8b2d49d443800a5f18508cd811fea3181723922a8a82b07", size = 5972659 },
            { url = "https://files.pythonhosted.org/packages/19/77/538f202862b9183f54108557bfda67e17603fc560c384559e769321c9d92/numpy-1.26.4-cp310-cp310-win_amd64.whl", hash = "sha256:b97fe8060236edf3662adfc2c633f56a08ae30560c56310562cb4f95500022d5", size = 15808905 },
            { url = "https://files.pythonhosted.org/packages/11/57/baae43d14fe163fa0e4c47f307b6b2511ab8d7d30177c491960504252053/numpy-1.26.4-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:4c66707fabe114439db9068ee468c26bbdf909cac0fb58686a42a24de1760c71", size = 20630554 },
            { url = "https://files.pythonhosted.org/packages/1a/2e/151484f49fd03944c4a3ad9c418ed193cfd02724e138ac8a9505d056c582/numpy-1.26.4-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:edd8b5fe47dab091176d21bb6de568acdd906d1887a4584a15a9a96a1dca06ef", size = 13997127 },
            { url = "https://files.pythonhosted.org/packages/79/ae/7e5b85136806f9dadf4878bf73cf223fe5c2636818ba3ab1c585d0403164/numpy-1.26.4-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:7ab55401287bfec946ced39700c053796e7cc0e3acbef09993a9ad2adba6ca6e", size = 14222994 },
            { url = "https://files.pythonhosted.org/packages/3a/d0/edc009c27b406c4f9cbc79274d6e46d634d139075492ad055e3d68445925/numpy-1.26.4-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:666dbfb6ec68962c033a450943ded891bed2d54e6755e35e5835d63f4f6931d5", size = 18252005 },
            { url = "https://files.pythonhosted.org/packages/09/bf/2b1aaf8f525f2923ff6cfcf134ae5e750e279ac65ebf386c75a0cf6da06a/numpy-1.26.4-cp311-cp311-musllinux_1_1_aarch64.whl", hash = "sha256:96ff0b2ad353d8f990b63294c8986f1ec3cb19d749234014f4e7eb0112ceba5a", size = 13885297 },
            { url = "https://files.pythonhosted.org/packages/df/a0/4e0f14d847cfc2a633a1c8621d00724f3206cfeddeb66d35698c4e2cf3d2/numpy-1.26.4-cp311-cp311-musllinux_1_1_x86_64.whl", hash = "sha256:60dedbb91afcbfdc9bc0b1f3f402804070deed7392c23eb7a7f07fa857868e8a", size = 18093567 },
            { url = "https://files.pythonhosted.org/packages/d2/b7/a734c733286e10a7f1a8ad1ae8c90f2d33bf604a96548e0a4a3a6739b468/numpy-1.26.4-cp311-cp311-win32.whl", hash = "sha256:1af303d6b2210eb850fcf03064d364652b7120803a0b872f5211f5234b399f20", size = 5968812 },
            { url = "https://files.pythonhosted.org/packages/3f/6b/5610004206cf7f8e7ad91c5a85a8c71b2f2f8051a0c0c4d5916b76d6cbb2/numpy-1.26.4-cp311-cp311-win_amd64.whl", hash = "sha256:cd25bcecc4974d09257ffcd1f098ee778f7834c3ad767fe5db785be9a4aa9cb2", size = 15811913 },
            { url = "https://files.pythonhosted.org/packages/95/12/8f2020a8e8b8383ac0177dc9570aad031a3beb12e38847f7129bacd96228/numpy-1.26.4-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:b3ce300f3644fb06443ee2222c2201dd3a89ea6040541412b8fa189341847218", size = 20335901 },
            { url = "https://files.pythonhosted.org/packages/75/5b/ca6c8bd14007e5ca171c7c03102d17b4f4e0ceb53957e8c44343a9546dcc/numpy-1.26.4-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:03a8c78d01d9781b28a6989f6fa1bb2c4f2d51201cf99d3dd875df6fbd96b23b", size = 13685868 },
            { url = "https://files.pythonhosted.org/packages/79/f8/97f10e6755e2a7d027ca783f63044d5b1bc1ae7acb12afe6a9b4286eac17/numpy-1.26.4-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:9fad7dcb1aac3c7f0584a5a8133e3a43eeb2fe127f47e3632d43d677c66c102b", size = 13925109 },
            { url = "https://files.pythonhosted.org/packages/0f/50/de23fde84e45f5c4fda2488c759b69990fd4512387a8632860f3ac9cd225/numpy-1.26.4-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:675d61ffbfa78604709862923189bad94014bef562cc35cf61d3a07bba02a7ed", size = 17950613 },
            { url = "https://files.pythonhosted.org/packages/4c/0c/9c603826b6465e82591e05ca230dfc13376da512b25ccd0894709b054ed0/numpy-1.26.4-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:ab47dbe5cc8210f55aa58e4805fe224dac469cde56b9f731a4c098b91917159a", size = 13572172 },
            { url = "https://files.pythonhosted.org/packages/76/8c/2ba3902e1a0fc1c74962ea9bb33a534bb05984ad7ff9515bf8d07527cadd/numpy-1.26.4-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:1dda2e7b4ec9dd512f84935c5f126c8bd8b9f2fc001e9f54af255e8c5f16b0e0", size = 17786643 },
            { url = "https://files.pythonhosted.org/packages/28/4a/46d9e65106879492374999e76eb85f87b15328e06bd1550668f79f7b18c6/numpy-1.26.4-cp312-cp312-win32.whl", hash = "sha256:50193e430acfc1346175fcbdaa28ffec49947a06918b7b92130744e81e640110", size = 5677803 },
            { url = "https://files.pythonhosted.org/packages/16/2e/86f24451c2d530c88daf997cb8d6ac622c1d40d19f5a031ed68a4b73a374/numpy-1.26.4-cp312-cp312-win_amd64.whl", hash = "sha256:08beddf13648eb95f8d867350f6a018a4be2e5ad54c8d8caed89ebca558b2818", size = 15517754 },
            { url = "https://files.pythonhosted.org/packages/7d/24/ce71dc08f06534269f66e73c04f5709ee024a1afe92a7b6e1d73f158e1f8/numpy-1.26.4-cp39-cp39-macosx_10_9_x86_64.whl", hash = "sha256:7349ab0fa0c429c82442a27a9673fc802ffdb7c7775fad780226cb234965e53c", size = 20636301 },
            { url = "https://files.pythonhosted.org/packages/ae/8c/ab03a7c25741f9ebc92684a20125fbc9fc1b8e1e700beb9197d750fdff88/numpy-1.26.4-cp39-cp39-macosx_11_0_arm64.whl", hash = "sha256:52b8b60467cd7dd1e9ed082188b4e6bb35aa5cdd01777621a1658910745b90be", size = 13971216 },
            { url = "https://files.pythonhosted.org/packages/6d/64/c3bcdf822269421d85fe0d64ba972003f9bb4aa9a419da64b86856c9961f/numpy-1.26.4-cp39-cp39-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:d5241e0a80d808d70546c697135da2c613f30e28251ff8307eb72ba696945764", size = 14226281 },
            { url = "https://files.pythonhosted.org/packages/54/30/c2a907b9443cf42b90c17ad10c1e8fa801975f01cb9764f3f8eb8aea638b/numpy-1.26.4-cp39-cp39-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f870204a840a60da0b12273ef34f7051e98c3b5961b61b0c2c1be6dfd64fbcd3", size = 18249516 },
            { url = "https://files.pythonhosted.org/packages/43/12/01a563fc44c07095996d0129b8899daf89e4742146f7044cdbdb3101c57f/numpy-1.26.4-cp39-cp39-musllinux_1_1_aarch64.whl", hash = "sha256:679b0076f67ecc0138fd2ede3a8fd196dddc2ad3254069bcb9faf9a79b1cebcd", size = 13882132 },
            { url = "https://files.pythonhosted.org/packages/16/ee/9df80b06680aaa23fc6c31211387e0db349e0e36d6a63ba3bd78c5acdf11/numpy-1.26.4-cp39-cp39-musllinux_1_1_x86_64.whl", hash = "sha256:47711010ad8555514b434df65f7d7b076bb8261df1ca9bb78f53d3b2db02e95c", size = 18084181 },
            { url = "https://files.pythonhosted.org/packages/28/7d/4b92e2fe20b214ffca36107f1a3e75ef4c488430e64de2d9af5db3a4637d/numpy-1.26.4-cp39-cp39-win32.whl", hash = "sha256:a354325ee03388678242a4d7ebcd08b5c727033fcff3b2f536aea978e15ee9e6", size = 5976360 },
            { url = "https://files.pythonhosted.org/packages/b5/42/054082bd8220bbf6f297f982f0a8f5479fcbc55c8b511d928df07b965869/numpy-1.26.4-cp39-cp39-win_amd64.whl", hash = "sha256:3373d5d70a5fe74a2c1bb6d2cfd9609ecf686d47a2d7b1d37a8f3b6bf6003aea", size = 15814633 },
            { url = "https://files.pythonhosted.org/packages/3f/72/3df6c1c06fc83d9cfe381cccb4be2532bbd38bf93fbc9fad087b6687f1c0/numpy-1.26.4-pp39-pypy39_pp73-macosx_10_9_x86_64.whl", hash = "sha256:afedb719a9dcfc7eaf2287b839d8198e06dcd4cb5d276a3df279231138e83d30", size = 20455961 },
            { url = "https://files.pythonhosted.org/packages/8e/02/570545bac308b58ffb21adda0f4e220ba716fb658a63c151daecc3293350/numpy-1.26.4-pp39-pypy39_pp73-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:95a7476c59002f2f6c590b9b7b998306fba6a5aa646b1e22ddfeaf8f78c3a29c", size = 18061071 },
            { url = "https://files.pythonhosted.org/packages/f4/5f/fafd8c51235f60d49f7a88e2275e13971e90555b67da52dd6416caec32fe/numpy-1.26.4-pp39-pypy39_pp73-win_amd64.whl", hash = "sha256:7e50d0a0cc3189f9cb0aeb3a6a6af18c16f59f004b866cd2be1c14b36134a4a0", size = 15709730 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "numpy", version = "1.24.4", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version < '3.9'" },
            { name = "numpy", version = "1.26.4", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version >= '3.9'" },
        ]

        [package.metadata]
        requires-dist = [{ name = "numpy" }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    Ok(())
}

#[test]
fn lock_requires_python_fewest_versions() -> Result<()> {
    let context = TestContext::new("3.11");

    let lockfile = context.temp_dir.join("uv.lock");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.8"
        dependencies = ["numpy"]

        [tool.uv]
        fork-strategy = "fewest"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(&lockfile).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.8"

        [options]
        fork-strategy = "fewest"
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "numpy"
        version = "1.24.4"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a4/9b/027bec52c633f6556dba6b722d9a0befb40498b9ceddd29cbe67a45a127c/numpy-1.24.4.tar.gz", hash = "sha256:80f5e3a4e498641401868df4208b74581206afbee7cf7b8329daae82676d9463", size = 10911229 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/6b/80/6cdfb3e275d95155a34659163b83c09e3a3ff9f1456880bec6cc63d71083/numpy-1.24.4-cp310-cp310-macosx_10_9_x86_64.whl", hash = "sha256:c0bfb52d2169d58c1cdb8cc1f16989101639b34c7d3ce60ed70b19c63eba0b64", size = 19789140 },
            { url = "https://files.pythonhosted.org/packages/64/5f/3f01d753e2175cfade1013eea08db99ba1ee4bdb147ebcf3623b75d12aa7/numpy-1.24.4-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:ed094d4f0c177b1b8e7aa9cba7d6ceed51c0e569a5318ac0ca9a090680a6a1b1", size = 13854297 },
            { url = "https://files.pythonhosted.org/packages/5a/b3/2f9c21d799fa07053ffa151faccdceeb69beec5a010576b8991f614021f7/numpy-1.24.4-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:79fc682a374c4a8ed08b331bef9c5f582585d1048fa6d80bc6c35bc384eee9b4", size = 13995611 },
            { url = "https://files.pythonhosted.org/packages/10/be/ae5bf4737cb79ba437879915791f6f26d92583c738d7d960ad94e5c36adf/numpy-1.24.4-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:7ffe43c74893dbf38c2b0a1f5428760a1a9c98285553c89e12d70a96a7f3a4d6", size = 17282357 },
            { url = "https://files.pythonhosted.org/packages/c0/64/908c1087be6285f40e4b3e79454552a701664a079321cff519d8c7051d06/numpy-1.24.4-cp310-cp310-win32.whl", hash = "sha256:4c21decb6ea94057331e111a5bed9a79d335658c27ce2adb580fb4d54f2ad9bc", size = 12429222 },
            { url = "https://files.pythonhosted.org/packages/22/55/3d5a7c1142e0d9329ad27cece17933b0e2ab4e54ddc5c1861fbfeb3f7693/numpy-1.24.4-cp310-cp310-win_amd64.whl", hash = "sha256:b4bea75e47d9586d31e892a7401f76e909712a0fd510f58f5337bea9572c571e", size = 14841514 },
            { url = "https://files.pythonhosted.org/packages/a9/cc/5ed2280a27e5dab12994c884f1f4d8c3bd4d885d02ae9e52a9d213a6a5e2/numpy-1.24.4-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:f136bab9c2cfd8da131132c2cf6cc27331dd6fae65f95f69dcd4ae3c3639c810", size = 19775508 },
            { url = "https://files.pythonhosted.org/packages/c0/bc/77635c657a3668cf652806210b8662e1aff84b818a55ba88257abf6637a8/numpy-1.24.4-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:e2926dac25b313635e4d6cf4dc4e51c8c0ebfed60b801c799ffc4c32bf3d1254", size = 13840033 },
            { url = "https://files.pythonhosted.org/packages/a7/4c/96cdaa34f54c05e97c1c50f39f98d608f96f0677a6589e64e53104e22904/numpy-1.24.4-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:222e40d0e2548690405b0b3c7b21d1169117391c2e82c378467ef9ab4c8f0da7", size = 13991951 },
            { url = "https://files.pythonhosted.org/packages/22/97/dfb1a31bb46686f09e68ea6ac5c63fdee0d22d7b23b8f3f7ea07712869ef/numpy-1.24.4-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:7215847ce88a85ce39baf9e89070cb860c98fdddacbaa6c0da3ffb31b3350bd5", size = 17278923 },
            { url = "https://files.pythonhosted.org/packages/35/e2/76a11e54139654a324d107da1d98f99e7aa2a7ef97cfd7c631fba7dbde71/numpy-1.24.4-cp311-cp311-win32.whl", hash = "sha256:4979217d7de511a8d57f4b4b5b2b965f707768440c17cb70fbf254c4b225238d", size = 12422446 },
            { url = "https://files.pythonhosted.org/packages/d8/ec/ebef2f7d7c28503f958f0f8b992e7ce606fb74f9e891199329d5f5f87404/numpy-1.24.4-cp311-cp311-win_amd64.whl", hash = "sha256:b7b1fc9864d7d39e28f41d089bfd6353cb5f27ecd9905348c24187a768c79694", size = 14834466 },
            { url = "https://files.pythonhosted.org/packages/11/10/943cfb579f1a02909ff96464c69893b1d25be3731b5d3652c2e0cf1281ea/numpy-1.24.4-cp38-cp38-macosx_10_9_x86_64.whl", hash = "sha256:1452241c290f3e2a312c137a9999cdbf63f78864d63c79039bda65ee86943f61", size = 19780722 },
            { url = "https://files.pythonhosted.org/packages/a7/ae/f53b7b265fdc701e663fbb322a8e9d4b14d9cb7b2385f45ddfabfc4327e4/numpy-1.24.4-cp38-cp38-macosx_11_0_arm64.whl", hash = "sha256:04640dab83f7c6c85abf9cd729c5b65f1ebd0ccf9de90b270cd61935eef0197f", size = 13843102 },
            { url = "https://files.pythonhosted.org/packages/25/6f/2586a50ad72e8dbb1d8381f837008a0321a3516dfd7cb57fc8cf7e4bb06b/numpy-1.24.4-cp38-cp38-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:a5425b114831d1e77e4b5d812b69d11d962e104095a5b9c3b641a218abcc050e", size = 14039616 },
            { url = "https://files.pythonhosted.org/packages/98/5d/5738903efe0ecb73e51eb44feafba32bdba2081263d40c5043568ff60faf/numpy-1.24.4-cp38-cp38-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:dd80e219fd4c71fc3699fc1dadac5dcf4fd882bfc6f7ec53d30fa197b8ee22dc", size = 17316263 },
            { url = "https://files.pythonhosted.org/packages/d1/57/8d328f0b91c733aa9aa7ee540dbc49b58796c862b4fbcb1146c701e888da/numpy-1.24.4-cp38-cp38-win32.whl", hash = "sha256:4602244f345453db537be5314d3983dbf5834a9701b7723ec28923e2889e0bb2", size = 12455660 },
            { url = "https://files.pythonhosted.org/packages/69/65/0d47953afa0ad569d12de5f65d964321c208492064c38fe3b0b9744f8d44/numpy-1.24.4-cp38-cp38-win_amd64.whl", hash = "sha256:692f2e0f55794943c5bfff12b3f56f99af76f902fc47487bdfe97856de51a706", size = 14868112 },
            { url = "https://files.pythonhosted.org/packages/9a/cd/d5b0402b801c8a8b56b04c1e85c6165efab298d2f0ab741c2406516ede3a/numpy-1.24.4-cp39-cp39-macosx_10_9_x86_64.whl", hash = "sha256:2541312fbf09977f3b3ad449c4e5f4bb55d0dbf79226d7724211acc905049400", size = 19816549 },
            { url = "https://files.pythonhosted.org/packages/14/27/638aaa446f39113a3ed38b37a66243e21b38110d021bfcb940c383e120f2/numpy-1.24.4-cp39-cp39-macosx_11_0_arm64.whl", hash = "sha256:9667575fb6d13c95f1b36aca12c5ee3356bf001b714fc354eb5465ce1609e62f", size = 13879950 },
            { url = "https://files.pythonhosted.org/packages/8f/27/91894916e50627476cff1a4e4363ab6179d01077d71b9afed41d9e1f18bf/numpy-1.24.4-cp39-cp39-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:f3a86ed21e4f87050382c7bc96571755193c4c1392490744ac73d660e8f564a9", size = 14030228 },
            { url = "https://files.pythonhosted.org/packages/7a/7c/d7b2a0417af6428440c0ad7cb9799073e507b1a465f827d058b826236964/numpy-1.24.4-cp39-cp39-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:d11efb4dbecbdf22508d55e48d9c8384db795e1b7b51ea735289ff96613ff74d", size = 17311170 },
            { url = "https://files.pythonhosted.org/packages/18/9d/e02ace5d7dfccee796c37b995c63322674daf88ae2f4a4724c5dd0afcc91/numpy-1.24.4-cp39-cp39-win32.whl", hash = "sha256:6620c0acd41dbcb368610bb2f4d83145674040025e5536954782467100aa8835", size = 12454918 },
            { url = "https://files.pythonhosted.org/packages/63/38/6cc19d6b8bfa1d1a459daf2b3fe325453153ca7019976274b6f33d8b5663/numpy-1.24.4-cp39-cp39-win_amd64.whl", hash = "sha256:befe2bf740fd8373cf56149a5c23a0f601e82869598d41f8e188a0e9869926f8", size = 14867441 },
            { url = "https://files.pythonhosted.org/packages/a4/fd/8dff40e25e937c94257455c237b9b6bf5a30d42dd1cc11555533be099492/numpy-1.24.4-pp38-pypy38_pp73-macosx_10_9_x86_64.whl", hash = "sha256:31f13e25b4e304632a4619d0e0777662c2ffea99fcae2029556b17d8ff958aef", size = 19156590 },
            { url = "https://files.pythonhosted.org/packages/42/e7/4bf953c6e05df90c6d351af69966384fed8e988d0e8c54dad7103b59f3ba/numpy-1.24.4-pp38-pypy38_pp73-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:95f7ac6540e95bc440ad77f56e520da5bf877f87dca58bd095288dce8940532a", size = 16705744 },
            { url = "https://files.pythonhosted.org/packages/fc/dd/9106005eb477d022b60b3817ed5937a43dad8fd1f20b0610ea8a32fcb407/numpy-1.24.4-pp38-pypy38_pp73-win_amd64.whl", hash = "sha256:e98f220aa76ca2a977fe435f5b04d7b3470c0a2e6312907b37ba6068f26787f2", size = 14734290 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "numpy" },
        ]

        [package.metadata]
        requires-dist = [{ name = "numpy" }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    Ok(())
}

/// Ensure that `python_version >= '3.10' or python_version < '3.10'` is correctly collapsed to
/// the full version range. This is _not_ the case under standard PEP 440 semantics, but Python
/// requirements are evaluated using release-only semantics.
///
/// However, `python_full_version` should use PubGrub semantics, as (e.g.)
/// `python_full_version >= '3.10' or python_full_version < '3.10'` will actually exclude versions
/// like `3.10.0b0`.
#[test]
fn lock_python_version_marker_complement() -> Result<()> {
    let context = TestContext::new("3.11");

    let lockfile = context.temp_dir.join("uv.lock");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.8"
        dependencies = [
            "iniconfig ; python_version >= '3.10'",
            "iniconfig ; python_version < '3.10'",
            "attrs ; python_version > '3.10'",
            "attrs ; python_version <= '3.10'",
            "typing-extensions ; python_full_version > '3.10'",
            "typing-extensions ; python_full_version <= '3.10'",
        ]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(&lockfile).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.8"
        resolution-markers = [
            "python_full_version >= '3.11'",
            "python_full_version > '3.10' and python_full_version < '3.11'",
            "python_full_version == '3.10'",
            "python_full_version < '3.10'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "attrs"
        version = "23.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/e3/fc/f800d51204003fa8ae392c4e8278f256206e7a919b708eef054f5f4b650d/attrs-23.2.0.tar.gz", hash = "sha256:935dc3b529c262f6cf76e50877d35a4bd3c1de194fd41f47a2b7ae8f19971f30", size = 780820 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e0/44/827b2a91a5816512fcaf3cc4ebc465ccd5d598c45cefa6703fcf4a79018f/attrs-23.2.0-py3-none-any.whl", hash = "sha256:99b87a485a5820b23b879f04c2305b44b951b502fd64be915879d77a7e8fc6f1", size = 60752 },
        ]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "attrs" },
            { name = "iniconfig" },
            { name = "typing-extensions" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "attrs", marker = "python_full_version < '3.11'" },
            { name = "attrs", marker = "python_full_version >= '3.11'" },
            { name = "iniconfig", marker = "python_full_version < '3.10'" },
            { name = "iniconfig", marker = "python_full_version >= '3.10'" },
            { name = "typing-extensions", marker = "python_full_version <= '3.10'" },
            { name = "typing-extensions", marker = "python_full_version > '3.10'" },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    Ok(())
}

/// Lock the development dependencies for a project.
#[test]
fn lock_dev() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [tool.uv]
        dev-dependencies = ["typing-extensions @ https://files.pythonhosted.org/packages/26/9f/ad63fc0248c5379346306f8668cda6e2e2e9c95e01216d2b8ffd9ff037d0/typing_extensions-4.12.2-py3-none-any.whl"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.dev-dependencies]
        dev = [
            { name = "typing-extensions" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig" }]

        [package.metadata.requires-dev]
        dev = [{ name = "typing-extensions", url = "https://files.pythonhosted.org/packages/26/9f/ad63fc0248c5379346306f8668cda6e2e2e9c95e01216d2b8ffd9ff037d0/typing_extensions-4.12.2-py3-none-any.whl" }]

        [[package]]
        name = "typing-extensions"
        version = "4.12.2"
        source = { url = "https://files.pythonhosted.org/packages/26/9f/ad63fc0248c5379346306f8668cda6e2e2e9c95e01216d2b8ffd9ff037d0/typing_extensions-4.12.2-py3-none-any.whl" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/26/9f/ad63fc0248c5379346306f8668cda6e2e2e9c95e01216d2b8ffd9ff037d0/typing_extensions-4.12.2-py3-none-any.whl", hash = "sha256:04e5ca0351e0f3f85c6853954072df659d0d13fac324d0072316b67d7794700d" },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    // Install from the lockfile, excluding development dependencies.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--no-dev"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     + project==0.1.0 (from file://[TEMP_DIR]/)
    "###);

    // Install from the lockfile, including development dependencies (the default).
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 1 package in [TIME]
     + typing-extensions==4.12.2 (from https://files.pythonhosted.org/packages/26/9f/ad63fc0248c5379346306f8668cda6e2e2e9c95e01216d2b8ffd9ff037d0/typing_extensions-4.12.2-py3-none-any.whl)
    "###);

    Ok(())
}

/// Lock a package that's included both conditionally and unconditionally in the lockfile.
#[test]
fn lock_conditional_unconditional() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig", "iniconfig ; python_version < '3.12'"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "iniconfig" },
            { name = "iniconfig", marker = "python_full_version < '3.12'" },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    Ok(())
}

/// Lock a package that's included twice with different markers.
#[test]
fn lock_multiple_markers() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig ; implementation_name == 'cpython'", "iniconfig ; python_version < '3.12'"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig", marker = "implementation_name == 'cpython'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "iniconfig", marker = "python_full_version < '3.12'" },
            { name = "iniconfig", marker = "implementation_name == 'cpython'" },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    Ok(())
}

/// Check relative and absolute path handling in lockfiles.
#[test]
fn lock_relative_and_absolute_paths() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc! {r#"
        [project]
        name = "a"
        version = "0.1.0"
        requires-python = ">=3.11,<3.13"
        dependencies = ["b", "c"]

        [tool.uv.sources]
        b = {{ path = "b" }}
        c = {{ path = '{}' }}

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
        context.temp_dir.join("c").display()
    })?;
    context.temp_dir.child("a/__init__.py").touch()?;
    context
        .temp_dir
        .child("b/pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "b"
        version = "0.1.0"
        dependencies = []
        requires-python = ">=3.11,<3.13"
        license = {text = "MIT"}

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

    "#})?;
    context.temp_dir.child("b/b/__init__.py").touch()?;
    context
        .temp_dir
        .child("c/pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "c"
        version = "0.1.0"
        dependencies = []
        requires-python = ">=3.11,<3.13"
        license = {text = "MIT"}

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

    "#})?;
    context.temp_dir.child("c/c/__init__.py").touch()?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 3 packages in [TIME]
        "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.11, <3.13"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "a"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "b" },
            { name = "c" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "b", directory = "b" },
            { name = "c", directory = "c" },
        ]

        [[package]]
        name = "b"
        version = "0.1.0"
        source = { directory = "b" }

        [[package]]
        name = "c"
        version = "0.1.0"
        source = { directory = "c" }
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    Ok(())
}

/// Lock a project that includes cyclic dependencies.
#[test]
fn lock_cycles() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["testtools==2.3.0", "fixtures==3.0.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 11 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "argparse"
        version = "1.4.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/18/dd/e617cfc3f6210ae183374cd9f6a26b20514bbb5a792af97949c5aacddf0f/argparse-1.4.0.tar.gz", hash = "sha256:62b089a55be1d8949cd2bc7e0df0bddb9e028faefc8c32038cc84862aefdd6e4", size = 70508 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f2/94/3af39d34be01a24a6e65433d19e107099374224905f1e0cc6bbe1fd22a2f/argparse-1.4.0-py2.py3-none-any.whl", hash = "sha256:c31647edb69fd3d465a847ea3157d37bed1f95f19760b11a47aa91c04b666314", size = 23000 },
        ]

        [[package]]
        name = "extras"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/be/18/0b7283f0ebf6ad4bb6b9937538495eadf05ef097b102946b9445c4242636/extras-1.0.0.tar.gz", hash = "sha256:132e36de10b9c91d5d4cc620160a476e0468a88f16c9431817a6729611a81b4e", size = 6759 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/03/e9/e915af1f97914cd0bc021e125fd1bfd4106de614a275e4b6866dd9a209ac/extras-1.0.0-py2.py3-none-any.whl", hash = "sha256:f689f08df47e2decf76aa6208c081306e7bd472630eb1ec8a875c67de2366e87", size = 7279 },
        ]

        [[package]]
        name = "fixtures"
        version = "3.0.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "pbr" },
            { name = "six" },
            { name = "testtools" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/84/be/94ecbc3f2487bd14aa8b44852f498086219b7cc0c8250ee65a03e2c63119/fixtures-3.0.0.tar.gz", hash = "sha256:fcf0d60234f1544da717a9738325812de1f42c2fa085e2d9252d8fff5712b2ef", size = 56629 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a8/28/7eed6bf76792f418029a18d5b2ace87ce7562927cdd00f1cefe481cd148f/fixtures-3.0.0-py2.py3-none-any.whl", hash = "sha256:2a551b0421101de112d9497fb5f6fd25e5019391c0fbec9bad591ecae981420d", size = 67478 },
        ]

        [[package]]
        name = "linecache2"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/44/b0/963c352372c242f9e40db02bbc6a39ae51bde15dddee8523fe4aca94a97e/linecache2-1.0.0.tar.gz", hash = "sha256:4b26ff4e7110db76eeb6f5a7b64a82623839d595c2038eeda662f2a2db78e97c", size = 11013 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c7/a3/c5da2a44c85bfbb6eebcfc1dde24933f8704441b98fdde6528f4831757a6/linecache2-1.0.0-py2.py3-none-any.whl", hash = "sha256:e78be9c0a0dfcbac712fe04fbf92b96cddae80b1b842f24248214c8496f006ef", size = 12967 },
        ]

        [[package]]
        name = "pbr"
        version = "6.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/8d/c2/ee43b3b11bf2b40e56536183fc9f22afbb04e882720332b6276ee2454c24/pbr-6.0.0.tar.gz", hash = "sha256:d1377122a5a00e2f940ee482999518efe16d745d423a670c27773dfbc3c9a7d9", size = 123150 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/64/dd/171c9fb653591cf265bcc89c436eec75c9bde3dec921cc236fa71e5698df/pbr-6.0.0-py2.py3-none-any.whl", hash = "sha256:4a7317d5e3b17a3dccb6a8cfe67dab65b20551404c52c8ed41279fa4f0cb4cda", size = 107506 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "fixtures" },
            { name = "testtools" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "fixtures", specifier = "==3.0.0" },
            { name = "testtools", specifier = "==2.3.0" },
        ]

        [[package]]
        name = "python-mimeparse"
        version = "1.6.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/0f/40/ac5f9e44a55b678c3cd881b4c3376e5b002677dbeab6fb3a50bac5d50d29/python-mimeparse-1.6.0.tar.gz", hash = "sha256:76e4b03d700a641fd7761d3cd4fdbbdcd787eade1ebfac43f877016328334f78", size = 6541 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/26/2e/03bce213a9bf02a2750dcb04e761785e9c763fc11071edc4b447eacbb842/python_mimeparse-1.6.0-py2.py3-none-any.whl", hash = "sha256:a295f03ff20341491bfe4717a39cd0a8cc9afad619ba44b77e86b0ab8a2b8282", size = 6057 },
        ]

        [[package]]
        name = "six"
        version = "1.16.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/71/39/171f1c67cd00715f190ba0b100d606d440a28c93c7714febeca8b79af85e/six-1.16.0.tar.gz", hash = "sha256:1e61c37477a1626458e36f7b1d82aa5c9b094fa4802892072e49de9c60c4c926", size = 34041 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d9/5a/e7c31adbe875f2abbb91bd84cf2dc52d792b5a01506781dbcf25c91daf11/six-1.16.0-py2.py3-none-any.whl", hash = "sha256:8abb2f1d86890a2dfb989f9a77cfcfd3e47c2a354b01111771326f8aa26e0254", size = 11053 },
        ]

        [[package]]
        name = "testtools"
        version = "2.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "extras" },
            { name = "fixtures" },
            { name = "pbr" },
            { name = "python-mimeparse" },
            { name = "six" },
            { name = "traceback2" },
            { name = "unittest2" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/e5/d4/9b22df94d0d5c83affe2517295c85fa2d9917f3cafa7dc7f6b1ce4135b00/testtools-2.3.0.tar.gz", hash = "sha256:5827ec6cf8233e0f29f51025addd713ca010061204fdea77484a2934690a0559", size = 231559 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/87/74/a4d55da28d7bba6d6f49430f22a62afd8472cb24a63fa61daef80d3e821b/testtools-2.3.0-py2.py3-none-any.whl", hash = "sha256:a2be448869171b6e0f26d9544088b8b98439ec180ce272040236d570a40bcbed", size = 184636 },
        ]

        [[package]]
        name = "traceback2"
        version = "1.4.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "linecache2" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/eb/7f/e20ba11390bdfc55117c8c6070838ec914e6f0053a602390a598057884eb/traceback2-1.4.0.tar.gz", hash = "sha256:05acc67a09980c2ecfedd3423f7ae0104839eccb55fc645773e1caa0951c3030", size = 15872 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/17/0a/6ac05a3723017a967193456a2efa0aa9ac4b51456891af1e2353bb9de21e/traceback2-1.4.0-py2.py3-none-any.whl", hash = "sha256:8253cebec4b19094d67cc5ed5af99bf1dba1285292226e98a31929f87a5d6b23", size = 16793 },
        ]

        [[package]]
        name = "unittest2"
        version = "1.1.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "argparse" },
            { name = "six" },
            { name = "traceback2" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/7f/c4/2b0e2d185d9d60772c10350d9853646832609d2f299a8300ab730f199db4/unittest2-1.1.0.tar.gz", hash = "sha256:22882a0e418c284e1f718a822b3b022944d53d2d908e1690b319a9d3eb2c0579", size = 81432 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/72/20/7f0f433060a962200b7272b8c12ba90ef5b903e218174301d0abfd523813/unittest2-1.1.0-py2.py3-none-any.whl", hash = "sha256:13f77d0875db6d9b435e1d4f41e74ad4cc2eb6e1d5c824996092b3430f088bb8", size = 96379 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 11 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 11 packages in [TIME]
    Installed 11 packages in [TIME]
     + argparse==1.4.0
     + extras==1.0.0
     + fixtures==3.0.0
     + linecache2==1.0.0
     + pbr==6.0.0
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + python-mimeparse==1.6.0
     + six==1.16.0
     + testtools==2.3.0
     + traceback2==1.4.0
     + unittest2==1.1.0
    "###);

    Ok(())
}

/// Ensures that stale lockfile metadata is detected.
#[test]
fn lock_new_extras() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["requests==2.31.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

    "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 6 packages in [TIME]
        "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "certifi"
        version = "2024.2.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/71/da/e94e26401b62acd6d91df2b52954aceb7f561743aa5ccc32152886c76c96/certifi-2024.2.2.tar.gz", hash = "sha256:0569859f95fc761b18b45ef421b1290a0f65f147e92a1e5eb3e635f9a5e4e66f", size = 164886 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ba/06/a07f096c664aeb9f01624f858c3add0a4e913d6c96257acb4fce61e7de14/certifi-2024.2.2-py3-none-any.whl", hash = "sha256:dc383c07b76109f368f6106eee2b593b04a011ea4d55f652c6ca24a754d1cdd1", size = 163774 },
        ]

        [[package]]
        name = "charset-normalizer"
        version = "3.3.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/63/09/c1bc53dab74b1816a00d8d030de5bf98f724c52c1635e07681d312f20be8/charset-normalizer-3.3.2.tar.gz", hash = "sha256:f30c3cb33b24454a82faecaf01b19c18562b1e89558fb6c56de4d9118a032fd5", size = 104809 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d1/b2/fcedc8255ec42afee97f9e6f0145c734bbe104aac28300214593eb326f1d/charset_normalizer-3.3.2-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:0b2b64d2bb6d3fb9112bafa732def486049e63de9618b5843bcdd081d8144cd8", size = 192892 },
            { url = "https://files.pythonhosted.org/packages/2e/7d/2259318c202f3d17f3fe6438149b3b9e706d1070fe3fcbb28049730bb25c/charset_normalizer-3.3.2-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:ddbb2551d7e0102e7252db79ba445cdab71b26640817ab1e3e3648dad515003b", size = 122213 },
            { url = "https://files.pythonhosted.org/packages/3a/52/9f9d17c3b54dc238de384c4cb5a2ef0e27985b42a0e5cc8e8a31d918d48d/charset_normalizer-3.3.2-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:55086ee1064215781fff39a1af09518bc9255b50d6333f2e4c74ca09fac6a8f6", size = 119404 },
            { url = "https://files.pythonhosted.org/packages/99/b0/9c365f6d79a9f0f3c379ddb40a256a67aa69c59609608fe7feb6235896e1/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:8f4a014bc36d3c57402e2977dada34f9c12300af536839dc38c0beab8878f38a", size = 137275 },
            { url = "https://files.pythonhosted.org/packages/91/33/749df346e93d7a30cdcb90cbfdd41a06026317bfbfb62cd68307c1a3c543/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:a10af20b82360ab00827f916a6058451b723b4e65030c5a18577c8b2de5b3389", size = 147518 },
            { url = "https://files.pythonhosted.org/packages/72/1a/641d5c9f59e6af4c7b53da463d07600a695b9824e20849cb6eea8a627761/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:8d756e44e94489e49571086ef83b2bb8ce311e730092d2c34ca8f7d925cb20aa", size = 140182 },
            { url = "https://files.pythonhosted.org/packages/ee/fb/14d30eb4956408ee3ae09ad34299131fb383c47df355ddb428a7331cfa1e/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:90d558489962fd4918143277a773316e56c72da56ec7aa3dc3dbbe20fdfed15b", size = 141869 },
            { url = "https://files.pythonhosted.org/packages/df/3e/a06b18788ca2eb6695c9b22325b6fde7dde0f1d1838b1792a0076f58fe9d/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:6ac7ffc7ad6d040517be39eb591cac5ff87416c2537df6ba3cba3bae290c0fed", size = 144042 },
            { url = "https://files.pythonhosted.org/packages/45/59/3d27019d3b447a88fe7e7d004a1e04be220227760264cc41b405e863891b/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:7ed9e526742851e8d5cc9e6cf41427dfc6068d4f5a3bb03659444b4cabf6bc26", size = 138275 },
            { url = "https://files.pythonhosted.org/packages/7b/ef/5eb105530b4da8ae37d506ccfa25057961b7b63d581def6f99165ea89c7e/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:8bdb58ff7ba23002a4c5808d608e4e6c687175724f54a5dade5fa8c67b604e4d", size = 144819 },
            { url = "https://files.pythonhosted.org/packages/a2/51/e5023f937d7f307c948ed3e5c29c4b7a3e42ed2ee0b8cdf8f3a706089bf0/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_ppc64le.whl", hash = "sha256:6b3251890fff30ee142c44144871185dbe13b11bab478a88887a639655be1068", size = 149415 },
            { url = "https://files.pythonhosted.org/packages/24/9d/2e3ef673dfd5be0154b20363c5cdcc5606f35666544381bee15af3778239/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_s390x.whl", hash = "sha256:b4a23f61ce87adf89be746c8a8974fe1c823c891d8f86eb218bb957c924bb143", size = 141212 },
            { url = "https://files.pythonhosted.org/packages/5b/ae/ce2c12fcac59cb3860b2e2d76dc405253a4475436b1861d95fe75bdea520/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:efcb3f6676480691518c177e3b465bcddf57cea040302f9f4e6e191af91174d4", size = 142167 },
            { url = "https://files.pythonhosted.org/packages/ed/3a/a448bf035dce5da359daf9ae8a16b8a39623cc395a2ffb1620aa1bce62b0/charset_normalizer-3.3.2-cp312-cp312-win32.whl", hash = "sha256:d965bba47ddeec8cd560687584e88cf699fd28f192ceb452d1d7ee807c5597b7", size = 93041 },
            { url = "https://files.pythonhosted.org/packages/b6/7c/8debebb4f90174074b827c63242c23851bdf00a532489fba57fef3416e40/charset_normalizer-3.3.2-cp312-cp312-win_amd64.whl", hash = "sha256:96b02a3dc4381e5494fad39be677abcb5e6634bf7b4fa83a6dd3112607547001", size = 100397 },
            { url = "https://files.pythonhosted.org/packages/28/76/e6222113b83e3622caa4bb41032d0b1bf785250607392e1b778aca0b8a7d/charset_normalizer-3.3.2-py3-none-any.whl", hash = "sha256:3e4d1f6587322d2788836a99c69062fbb091331ec940e02d12d179c1d53e25fc", size = 48543 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "requests" },
        ]

        [package.metadata]
        requires-dist = [{ name = "requests", specifier = "==2.31.0" }]

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
        sdist = { url = "https://files.pythonhosted.org/packages/9d/be/10918a2eac4ae9f02f6cfe6414b7a155ccd8f7f9d4380d62fd5b955065c3/requests-2.31.0.tar.gz", hash = "sha256:942c5a758f98d790eaed1a29cb6eefc7ffb0d1cf7af05c3d2791656dbd6ad1e1", size = 110794 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/70/8e/0e2d847013cb52cd35b38c009bb167a1a26b2ce6cd6965bf26b47bc0bf44/requests-2.31.0-py3-none-any.whl", hash = "sha256:58cd2187c01e70e6e26505bca751777aa9f2ee0b7f4300988b709f44e013003f", size = 62574 },
        ]

        [[package]]
        name = "urllib3"
        version = "2.2.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/7a/50/7fd50a27caa0652cd4caf224aa87741ea41d3265ad13f010886167cfcc79/urllib3-2.2.1.tar.gz", hash = "sha256:d0570876c61ab9e520d776c38acbbb5b05a776d3f9ff98a5c8fd5162a444cf19", size = 291020 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a2/73/a68704750a7679d0b6d3ad7aa8d4da8e14e151ae82e6fee774e6e0d05ec8/urllib3-2.2.1-py3-none-any.whl", hash = "sha256:450b20ec296a467077128bff42b73080516e71b56ff59a60a02bef2232c4fa9d", size = 121067 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "###);

    // Enable a new extra.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["requests[socks]==2.31.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

    "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 7 packages in [TIME]
        Added pysocks v1.7.1
        "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "certifi"
        version = "2024.2.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/71/da/e94e26401b62acd6d91df2b52954aceb7f561743aa5ccc32152886c76c96/certifi-2024.2.2.tar.gz", hash = "sha256:0569859f95fc761b18b45ef421b1290a0f65f147e92a1e5eb3e635f9a5e4e66f", size = 164886 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ba/06/a07f096c664aeb9f01624f858c3add0a4e913d6c96257acb4fce61e7de14/certifi-2024.2.2-py3-none-any.whl", hash = "sha256:dc383c07b76109f368f6106eee2b593b04a011ea4d55f652c6ca24a754d1cdd1", size = 163774 },
        ]

        [[package]]
        name = "charset-normalizer"
        version = "3.3.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/63/09/c1bc53dab74b1816a00d8d030de5bf98f724c52c1635e07681d312f20be8/charset-normalizer-3.3.2.tar.gz", hash = "sha256:f30c3cb33b24454a82faecaf01b19c18562b1e89558fb6c56de4d9118a032fd5", size = 104809 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d1/b2/fcedc8255ec42afee97f9e6f0145c734bbe104aac28300214593eb326f1d/charset_normalizer-3.3.2-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:0b2b64d2bb6d3fb9112bafa732def486049e63de9618b5843bcdd081d8144cd8", size = 192892 },
            { url = "https://files.pythonhosted.org/packages/2e/7d/2259318c202f3d17f3fe6438149b3b9e706d1070fe3fcbb28049730bb25c/charset_normalizer-3.3.2-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:ddbb2551d7e0102e7252db79ba445cdab71b26640817ab1e3e3648dad515003b", size = 122213 },
            { url = "https://files.pythonhosted.org/packages/3a/52/9f9d17c3b54dc238de384c4cb5a2ef0e27985b42a0e5cc8e8a31d918d48d/charset_normalizer-3.3.2-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:55086ee1064215781fff39a1af09518bc9255b50d6333f2e4c74ca09fac6a8f6", size = 119404 },
            { url = "https://files.pythonhosted.org/packages/99/b0/9c365f6d79a9f0f3c379ddb40a256a67aa69c59609608fe7feb6235896e1/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:8f4a014bc36d3c57402e2977dada34f9c12300af536839dc38c0beab8878f38a", size = 137275 },
            { url = "https://files.pythonhosted.org/packages/91/33/749df346e93d7a30cdcb90cbfdd41a06026317bfbfb62cd68307c1a3c543/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:a10af20b82360ab00827f916a6058451b723b4e65030c5a18577c8b2de5b3389", size = 147518 },
            { url = "https://files.pythonhosted.org/packages/72/1a/641d5c9f59e6af4c7b53da463d07600a695b9824e20849cb6eea8a627761/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:8d756e44e94489e49571086ef83b2bb8ce311e730092d2c34ca8f7d925cb20aa", size = 140182 },
            { url = "https://files.pythonhosted.org/packages/ee/fb/14d30eb4956408ee3ae09ad34299131fb383c47df355ddb428a7331cfa1e/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:90d558489962fd4918143277a773316e56c72da56ec7aa3dc3dbbe20fdfed15b", size = 141869 },
            { url = "https://files.pythonhosted.org/packages/df/3e/a06b18788ca2eb6695c9b22325b6fde7dde0f1d1838b1792a0076f58fe9d/charset_normalizer-3.3.2-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:6ac7ffc7ad6d040517be39eb591cac5ff87416c2537df6ba3cba3bae290c0fed", size = 144042 },
            { url = "https://files.pythonhosted.org/packages/45/59/3d27019d3b447a88fe7e7d004a1e04be220227760264cc41b405e863891b/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:7ed9e526742851e8d5cc9e6cf41427dfc6068d4f5a3bb03659444b4cabf6bc26", size = 138275 },
            { url = "https://files.pythonhosted.org/packages/7b/ef/5eb105530b4da8ae37d506ccfa25057961b7b63d581def6f99165ea89c7e/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:8bdb58ff7ba23002a4c5808d608e4e6c687175724f54a5dade5fa8c67b604e4d", size = 144819 },
            { url = "https://files.pythonhosted.org/packages/a2/51/e5023f937d7f307c948ed3e5c29c4b7a3e42ed2ee0b8cdf8f3a706089bf0/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_ppc64le.whl", hash = "sha256:6b3251890fff30ee142c44144871185dbe13b11bab478a88887a639655be1068", size = 149415 },
            { url = "https://files.pythonhosted.org/packages/24/9d/2e3ef673dfd5be0154b20363c5cdcc5606f35666544381bee15af3778239/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_s390x.whl", hash = "sha256:b4a23f61ce87adf89be746c8a8974fe1c823c891d8f86eb218bb957c924bb143", size = 141212 },
            { url = "https://files.pythonhosted.org/packages/5b/ae/ce2c12fcac59cb3860b2e2d76dc405253a4475436b1861d95fe75bdea520/charset_normalizer-3.3.2-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:efcb3f6676480691518c177e3b465bcddf57cea040302f9f4e6e191af91174d4", size = 142167 },
            { url = "https://files.pythonhosted.org/packages/ed/3a/a448bf035dce5da359daf9ae8a16b8a39623cc395a2ffb1620aa1bce62b0/charset_normalizer-3.3.2-cp312-cp312-win32.whl", hash = "sha256:d965bba47ddeec8cd560687584e88cf699fd28f192ceb452d1d7ee807c5597b7", size = 93041 },
            { url = "https://files.pythonhosted.org/packages/b6/7c/8debebb4f90174074b827c63242c23851bdf00a532489fba57fef3416e40/charset_normalizer-3.3.2-cp312-cp312-win_amd64.whl", hash = "sha256:96b02a3dc4381e5494fad39be677abcb5e6634bf7b4fa83a6dd3112607547001", size = 100397 },
            { url = "https://files.pythonhosted.org/packages/28/76/e6222113b83e3622caa4bb41032d0b1bf785250607392e1b778aca0b8a7d/charset_normalizer-3.3.2-py3-none-any.whl", hash = "sha256:3e4d1f6587322d2788836a99c69062fbb091331ec940e02d12d179c1d53e25fc", size = 48543 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "requests", extra = ["socks"] },
        ]

        [package.metadata]
        requires-dist = [{ name = "requests", extras = ["socks"], specifier = "==2.31.0" }]

        [[package]]
        name = "pysocks"
        version = "1.7.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bd/11/293dd436aea955d45fc4e8a35b6ae7270f5b8e00b53cf6c024c83b657a11/PySocks-1.7.1.tar.gz", hash = "sha256:3f8804571ebe159c380ac6de37643bb4685970655d3bba243530d6558b799aa0", size = 284429 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/8d/59/b4572118e098ac8e46e399a1dd0f2d85403ce8bbaad9ec79373ed6badaf9/PySocks-1.7.1-py3-none-any.whl", hash = "sha256:2725bd0a9925919b9b51739eea5f9e2bae91e83288108a9ad338b2e3a4435ee5", size = 16725 },
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
        sdist = { url = "https://files.pythonhosted.org/packages/9d/be/10918a2eac4ae9f02f6cfe6414b7a155ccd8f7f9d4380d62fd5b955065c3/requests-2.31.0.tar.gz", hash = "sha256:942c5a758f98d790eaed1a29cb6eefc7ffb0d1cf7af05c3d2791656dbd6ad1e1", size = 110794 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/70/8e/0e2d847013cb52cd35b38c009bb167a1a26b2ce6cd6965bf26b47bc0bf44/requests-2.31.0-py3-none-any.whl", hash = "sha256:58cd2187c01e70e6e26505bca751777aa9f2ee0b7f4300988b709f44e013003f", size = 62574 },
        ]

        [package.optional-dependencies]
        socks = [
            { name = "pysocks" },
        ]

        [[package]]
        name = "urllib3"
        version = "2.2.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/7a/50/7fd50a27caa0652cd4caf224aa87741ea41d3265ad13f010886167cfcc79/urllib3-2.2.1.tar.gz", hash = "sha256:d0570876c61ab9e520d776c38acbbb5b05a776d3f9ff98a5c8fd5162a444cf19", size = 291020 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a2/73/a68704750a7679d0b6d3ad7aa8d4da8e14e151ae82e6fee774e6e0d05ec8/urllib3-2.2.1-py3-none-any.whl", hash = "sha256:450b20ec296a467077128bff42b73080516e71b56ff59a60a02bef2232c4fa9d", size = 121067 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    "###);

    Ok(())
}

/// Ensure that the installer rejects invalid hashes from the lockfile.
///
/// In this case, the hashes for `idna` have all been incremented by one in the left-most digit.
#[test]
fn lock_invalid_hash() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    let lock = context.temp_dir.child("uv.lock");

    lock.write_str(r#"
        version = 1
        requires-python = ">=3.12"

        [[package]]
        name = "anyio"
        version = "3.7.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/c6/b3/fefbf7e78ab3b805dec67d698dc18dd505af7a18a8dd08868c9b4fa736b5/anyio-3.7.0.tar.gz", hash = "sha256:275d9973793619a5374e1c89a4f4ad3f4b0a5510a2b5b939444bee8f4c4d37ce", size = 142737 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/fe/7ce1926952c8a403b35029e194555558514b365ad77d75125f521a2bec62/anyio-3.7.0-py3-none-any.whl", hash = "sha256:eddca883c4175f14df8aedce21054bfca3adb70ffe76a9f607aef9d7fa2ea7f0", size = 80873 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:aecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:d05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio" },
        ]
        requires-dist = [{ name = "anyio", specifier = "==3.7.0" }]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "#)?;

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided. To update the lockfile, run `uv lock`.
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download `idna==3.6`
      ╰─▶ Hash mismatch for `idna==3.6`

          Expected:
            sha256:aecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca
            sha256:d05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f

          Computed:
            sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f
      help: `idna` (v3.6) was included because `project` (v0.1.0) depends on `anyio` (v3.7.0) which depends on `idna`
    "###);

    Ok(())
}

/// Vary the `--resolution-mode`, and ensure that the lockfile is updated.
#[test]
fn lock_resolution_mode() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio>=3"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
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
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio", specifier = ">=3" }]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    // Locking with `lowest-direct` should ignore the existing lockfile.
    uv_snapshot!(context.filters(), context.lock().arg("--resolution").arg("lowest-direct"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Ignoring existing lockfile due to change in resolution mode: `highest` vs. `lowest-direct`
    Resolved 4 packages in [TIME]
    Updated anyio v4.3.0 -> v3.0.0
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        resolution-mode = "lowest-direct"
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "3.0.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/99/0d/65165f99e5f4f3b4c43a5ed9db0fb7aa655f5a58f290727a30528a87eb45/anyio-3.0.0.tar.gz", hash = "sha256:b553598332c050af19f7d41f73a7790142f5bc3d5eb8bd82f5e515ec22019bd9", size = 116952 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/3b/49/ebee263b69fe243bd1fd0a88bc6bb0f7732bf1794ba3273cb446351f9482/anyio-3.0.0-py3-none-any.whl", hash = "sha256:e71c3d9d72291d12056c0265d07c6bbedf92332f78573e278aeb116f24f30395", size = 72182 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio", specifier = ">=3" }]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--resolution").arg("lowest-direct").arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    Ok(())
}

/// Lock a requirement from PyPI, filtering out wheels that target an ABI that is non-overlapping
/// with the `Requires-Python` constraint.
#[test]
fn lock_requires_python_no_wheels() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dearpygui==1.9.1"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because dearpygui==1.9.1 has no wheels with a matching Python version tag and your project depends on dearpygui==1.9.1, we can conclude that your project's requirements are unsatisfiable.
    "###);

    Ok(())
}

/// In this case, a package is included twice at the same version, but pointing to different direct
/// URLs.
#[test]
fn lock_same_version_multiple_urls() -> Result<()> {
    let context = TestContext::new("3.12");

    let v1 = context.temp_dir.child("v1");
    fs_err::create_dir_all(&v1)?;
    let pyproject_toml = v1.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "dependency"
        version = "0.0.1"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    let v2 = context.temp_dir.child("v2");
    fs_err::create_dir_all(&v2)?;
    let pyproject_toml = v2.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "dependency"
        version = "0.0.1"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.0.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc! {
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
          "dependency @ {} ; sys_platform == 'darwin'",
          "dependency @ {} ; sys_platform != 'darwin'",
        ]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
        Url::from_file_path(context.temp_dir.join("v1")).unwrap(),
        Url::from_file_path(context.temp_dir.join("v2")).unwrap(),
    })?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform == 'darwin'",
            "sys_platform != 'darwin'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "3.0.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "sys_platform != 'darwin'",
        ]
        dependencies = [
            { name = "idna", marker = "sys_platform != 'darwin'" },
            { name = "sniffio", marker = "sys_platform != 'darwin'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/99/0d/65165f99e5f4f3b4c43a5ed9db0fb7aa655f5a58f290727a30528a87eb45/anyio-3.0.0.tar.gz", hash = "sha256:b553598332c050af19f7d41f73a7790142f5bc3d5eb8bd82f5e515ec22019bd9", size = 116952 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/3b/49/ebee263b69fe243bd1fd0a88bc6bb0f7732bf1794ba3273cb446351f9482/anyio-3.0.0-py3-none-any.whl", hash = "sha256:e71c3d9d72291d12056c0265d07c6bbedf92332f78573e278aeb116f24f30395", size = 72182 },
        ]

        [[package]]
        name = "anyio"
        version = "3.7.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        dependencies = [
            { name = "idna", marker = "sys_platform == 'darwin'" },
            { name = "sniffio", marker = "sys_platform == 'darwin'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/c6/b3/fefbf7e78ab3b805dec67d698dc18dd505af7a18a8dd08868c9b4fa736b5/anyio-3.7.0.tar.gz", hash = "sha256:275d9973793619a5374e1c89a4f4ad3f4b0a5510a2b5b939444bee8f4c4d37ce", size = 142737 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/fe/7ce1926952c8a403b35029e194555558514b365ad77d75125f521a2bec62/anyio-3.7.0-py3-none-any.whl", hash = "sha256:eddca883c4175f14df8aedce21054bfca3adb70ffe76a9f607aef9d7fa2ea7f0", size = 80873 },
        ]

        [[package]]
        name = "dependency"
        version = "0.0.1"
        source = { directory = "v1" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        dependencies = [
            { name = "anyio", version = "3.7.0", source = { registry = "https://pypi.org/simple" }, marker = "sys_platform == 'darwin'" },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio", specifier = "==3.7.0" }]

        [[package]]
        name = "dependency"
        version = "0.0.1"
        source = { directory = "v2" }
        resolution-markers = [
            "sys_platform != 'darwin'",
        ]
        dependencies = [
            { name = "anyio", version = "3.0.0", source = { registry = "https://pypi.org/simple" }, marker = "sys_platform != 'darwin'" },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio", specifier = "==3.0.0" }]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "dependency", version = "0.0.1", source = { directory = "v1" }, marker = "sys_platform == 'darwin'" },
            { name = "dependency", version = "0.0.1", source = { directory = "v2" }, marker = "sys_platform != 'darwin'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "dependency", marker = "sys_platform != 'darwin'", directory = "v2" },
            { name = "dependency", marker = "sys_platform == 'darwin'", directory = "v1" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    "###);

    Ok(())
}

/// When locking with `--resolution-mode=lowest`, we shouldn't warn on unbounded direct
/// dependencies.
#[test]
fn lock_unsafe_lowest() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.optional-dependencies]
        dev = ["iniconfig"]
        all = ["project[dev]"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().arg("--resolution").arg("lowest-direct"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--resolution").arg("lowest-direct").arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    Ok(())
}

/// Lock a package that's excluded from the parent workspace, but depends on that parent.
#[test]
fn lock_exclusion() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.workspace]
        members = []
        exclude = ["child"]
        "#,
    )?;

    let child = context.temp_dir.child("child");
    fs_err::create_dir_all(&child)?;

    let pyproject_toml = child.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["project"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sources]
        project = { path = ".." }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().current_dir(&child), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(child.join("uv.lock")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "child"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "project" },
        ]

        [package.metadata]
        requires-dist = [{ name = "project", directory = "../" }]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { directory = "../" }
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Unable to find lockfile at `uv.lock`. To create a lockfile, run `uv lock` or `uv sync`.
    "###);

    Ok(())
}

/// See: <https://github.com/astral-sh/uv/issues/9832#issuecomment-2539121761>
#[test]
fn lock_relative_lock_deserialization() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        requires-python = ">=3.12"
        dependencies = ["member"]
        dynamic = ["version"]

        [tool.uv.sources]
        member = { workspace = true }

        [tool.uv.workspace]
        members = ["packages/*"]
        exclude = ["packages/child"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    let packages = context.temp_dir.child("packages");

    let member = packages.child("member");
    member.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "member"
        requires-python = ">=3.12"
        dependencies = []
        dynamic = ["version"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    let child = packages.child("child");
    child.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["member"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sources]
        member = { workspace = true }
        "#,
    )?;

    // Add an arbitrary lockfile, to ensure that we attempt to validate it, which is necessary to
    // trigger the bug.
    child.child("uv.lock").write_str(
        r#"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "child"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "project" },
        ]

        [package.metadata]
        requires-dist = [{ name = "project", directory = "../" }]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { directory = "../" }
    "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().current_dir(&child), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    error: Failed to generate package metadata for `child==0.1.0 @ editable+.`
      Caused by: Failed to parse entry: `member`
      Caused by: `member` references a workspace in `tool.uv.sources` (e.g., `member = { workspace = true }`), but is not a workspace member
    "###);

    Ok(())
}

/// Lock a workspace member with a non-workspace source.
#[test]
fn lock_non_workspace_source() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["child"]

        [tool.uv.workspace]
        members = ["child"]

        [tool.uv.sources]
        child = { path = "child" }
        "#,
    )?;

    let child = context.temp_dir.child("child");
    fs_err::create_dir_all(&child)?;

    let pyproject_toml = child.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().current_dir(&child), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `project @ file://[TEMP_DIR]/`
      ├─▶ Failed to parse entry: `child`
      ╰─▶ `child` is included as a workspace member, but references a path in `tool.uv.sources`. Workspace members must be declared as workspace sources (e.g., `child = { workspace = true }`).
    "###);

    Ok(())
}

/// Lock a workspace member with a non-workspace source.
#[test]
fn lock_no_workspace_source() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["child"]

        [tool.uv.workspace]
        members = ["child"]
        "#,
    )?;

    let child = context.temp_dir.child("child");
    fs_err::create_dir_all(&child)?;

    let pyproject_toml = child.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().current_dir(&child), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `project @ file://[TEMP_DIR]/`
      ├─▶ Failed to parse entry: `child`
      ╰─▶ `child` is included as a workspace member, but is missing an entry in `tool.uv.sources` (e.g., `child = { workspace = true }`)
    "###);

    Ok(())
}

/// Lock a workspace with a member that's a peer to the root.
#[test]
fn lock_peer_member() -> Result<()> {
    let context = TestContext::new("3.12");

    context
        .temp_dir
        .child("project")
        .child("pyproject.toml")
        .write_str(
            r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["child"]

        [tool.uv.workspace]
        members = ["../child"]

        [tool.uv.sources]
        child = { workspace = true }
        "#,
        )?;

    context
        .temp_dir
        .child("child")
        .child("pyproject.toml")
        .write_str(
            r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
        )?;

    uv_snapshot!(context.filters(), context.lock().current_dir(context.temp_dir.child("project")), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.child("project").child("uv.lock")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        members = [
            "child",
            "project",
        ]

        [[package]]
        name = "child"
        version = "0.1.0"
        source = { editable = "../child" }

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "child" },
        ]

        [package.metadata]
        requires-dist = [{ name = "child", editable = "../child" }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No `pyproject.toml` found in current directory or any parent directory
    "###);

    Ok(())
}

/// Ensure that development dependencies are omitted for non-workspace members. Below, `bar` depends
/// on `foo`, but `bar/uv.lock` should omit `anyio`, but should include `typing-extensions`.
#[test]
fn lock_dev_transitive() -> Result<()> {
    let context = TestContext::new("3.12");

    let foo = context.temp_dir.child("foo");
    fs_err::create_dir_all(&foo)?;

    let pyproject_toml = foo.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "foo"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        dev-dependencies = ["anyio"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    let bar = context.temp_dir.child("bar");
    fs_err::create_dir_all(&bar)?;

    let pyproject_toml = bar.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "bar"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["foo", "baz", "iniconfig>1"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sources]
        foo = { path = "../foo" }
        baz = { workspace = true }

        [tool.uv.workspace]
        members = ["baz"]
        "#,
    )?;

    let baz = bar.child("baz");
    fs_err::create_dir_all(&baz)?;

    let pyproject_toml = baz.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "baz"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv]
        dev-dependencies = ["typing-extensions>4"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().current_dir(&bar), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 5 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(bar.join("uv.lock")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        members = [
            "bar",
            "baz",
        ]

        [[package]]
        name = "bar"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "baz" },
            { name = "foo" },
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "baz", editable = "baz" },
            { name = "foo", directory = "../foo" },
            { name = "iniconfig", specifier = ">1" },
        ]

        [[package]]
        name = "baz"
        version = "0.1.0"
        source = { editable = "baz" }

        [package.dev-dependencies]
        dev = [
            { name = "typing-extensions" },
        ]

        [package.metadata]

        [package.metadata.requires-dev]
        dev = [{ name = "typing-extensions", specifier = ">4" }]

        [[package]]
        name = "foo"
        version = "0.1.0"
        source = { directory = "../foo" }

        [package.metadata]

        [package.metadata.requires-dev]
        dev = [{ name = "anyio" }]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: No `pyproject.toml` found in current directory or any parent directory
    "###);

    Ok(())
}

/// Avoid persisting registry credentials in `uv.lock`.
#[test]
fn lock_redact_https() -> Result<()> {
    // This test in particular seems to prompt a link mode warning
    // that occurs when hardlinking fails. In particular, in this test,
    // uv tries to hardlink between `/tmp` and `~/.local`, which on my
    // system fails because `/tmp` is a different file system (a ramdisk).
    // So we filter this warning out so that our snapshot tests pass on
    // systems where this warning pops up.
    //
    // This is caused by using `--no-cache` in some of the commands below,
    // which in turns means we don't use the test context cache location.
    // We should probably add a way to configure the `--no-cache` temporary
    // directory location during testing.
    let context = TestContext::new("3.12").with_filtered_link_mode_warning();

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "foo"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().arg("--index-url").arg("https://public:heron@pypi-proxy.fly.dev/basic-auth/simple"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    // The lockfile shout omit the credentials.
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "foo"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig" }]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi-proxy.fly.dev/basic-auth/simple" }
        sdist = { url = "https://pypi-proxy.fly.dev/basic-auth/files/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://pypi-proxy.fly.dev/basic-auth/files/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Installing from the lockfile should fail without credentials. Omit the root, so that we fail
    // when installing `iniconfig`, rather than when building `foo`.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--index-url").arg("https://pypi-proxy.fly.dev/basic-auth/simple").arg("--no-install-project"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download `iniconfig==2.0.0`
      ├─▶ Failed to fetch: `https://pypi-proxy.fly.dev/basic-auth/files/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl`
      ╰─▶ HTTP status client error (401 Unauthorized) for url (https://pypi-proxy.fly.dev/basic-auth/files/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl)
      help: `iniconfig` (v2.0.0) was included because `foo` (v0.1.0) depends on `iniconfig`
    "###);

    // Installing from the lockfile should fail without an index.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--no-install-project"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download `iniconfig==2.0.0`
      ├─▶ Failed to fetch: `https://pypi-proxy.fly.dev/basic-auth/files/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl`
      ╰─▶ HTTP status client error (401 Unauthorized) for url (https://pypi-proxy.fly.dev/basic-auth/files/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl)
      help: `iniconfig` (v2.0.0) was included because `foo` (v0.1.0) depends on `iniconfig`
    "###);

    // Installing from the lockfile should succeed when credentials are included on the command-line.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--index-url").arg("https://public:heron@pypi-proxy.fly.dev/basic-auth/simple"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + foo==0.1.0 (from file://[TEMP_DIR]/)
     + iniconfig==2.0.0
    "###);

    // A subsequent sync will succeed because the credentials are in uv's request cache.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--reinstall"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 2 packages in [TIME]
    Uninstalled 2 packages in [TIME]
    Installed 2 packages in [TIME]
     ~ foo==0.1.0 (from file://[TEMP_DIR]/)
     ~ iniconfig==2.0.0
    "###);

    // Installing without credentials will fail without a cache.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--reinstall").arg("--no-cache").arg("--no-install-project"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download `iniconfig==2.0.0`
      ├─▶ Failed to fetch: `https://pypi-proxy.fly.dev/basic-auth/files/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl`
      ╰─▶ HTTP status client error (401 Unauthorized) for url (https://pypi-proxy.fly.dev/basic-auth/files/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl)
      help: `iniconfig` (v2.0.0) was included because `foo` (v0.1.0) depends on `iniconfig`
    "###);

    // Installing with credentials from with `UV_INDEX_URL` should succeed.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--reinstall").arg("--no-cache").env(EnvVars::UV_INDEX_URL, "https://public:heron@pypi-proxy.fly.dev/basic-auth/simple"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 2 packages in [TIME]
    Uninstalled 2 packages in [TIME]
    Installed 2 packages in [TIME]
     ~ foo==0.1.0 (from file://[TEMP_DIR]/)
     ~ iniconfig==2.0.0
    "###);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "foo"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv]
        index-url = "https://public:heron@pypi-proxy.fly.dev/basic-auth/simple"
        "#,
    )?;

    // Installing from the lockfile should succeed when credentials are included via
    // `pyproject.toml`.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--reinstall").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 2 packages in [TIME]
    Uninstalled 2 packages in [TIME]
    Installed 2 packages in [TIME]
     ~ foo==0.1.0 (from file://[TEMP_DIR]/)
     ~ iniconfig==2.0.0
    "###);

    Ok(())
}

/// However, we don't currently avoid persisting Git credentials in `uv.lock`.
#[test]
fn lock_redact_git_pep508() -> Result<()> {
    let context = TestContext::new("3.12").with_filtered_link_mode_warning();
    let token = decode_token(common::READ_ONLY_GITHUB_TOKEN);

    let filters: Vec<_> = [(token.as_str(), "***")]
        .into_iter()
        .chain(context.filters())
        .collect();

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc! {
        r#"
        [project]
        name = "foo"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["uv-private-pypackage @ git+https://{token}@github.com/astral-test/uv-private-pypackage"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
        token = token,
    })?;

    uv_snapshot!(&filters, context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => filters.clone(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "foo"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "uv-private-pypackage" },
        ]

        [package.metadata]
        requires-dist = [{ name = "uv-private-pypackage", git = "https://github.com/astral-test/uv-private-pypackage" }]

        [[package]]
        name = "uv-private-pypackage"
        version = "0.1.0"
        source = { git = "https://github.com/astral-test/uv-private-pypackage#d780faf0ac91257d4d5a4f0c5a0e4509608c0071" }
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(&filters, context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(&filters, context.sync().arg("--frozen").arg("--reinstall").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + foo==0.1.0 (from file://[TEMP_DIR]/)
     + uv-private-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-private-pypackage@d780faf0ac91257d4d5a4f0c5a0e4509608c0071)
    "###);

    Ok(())
}

#[test]
fn lock_redact_git_sources() -> Result<()> {
    let context = TestContext::new("3.12").with_filtered_link_mode_warning();
    let token = decode_token(common::READ_ONLY_GITHUB_TOKEN);

    let filters: Vec<_> = [(token.as_str(), "***")]
        .into_iter()
        .chain(context.filters())
        .collect();

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc! {
        r#"
        [project]
        name = "foo"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["uv-private-pypackage"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sources]
        uv-private-pypackage = {{ git = "https://{token}@github.com/astral-test/uv-private-pypackage" }}
        "#,
        token = token,
    })?;

    uv_snapshot!(&filters, context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => filters.clone(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "foo"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "uv-private-pypackage" },
        ]

        [package.metadata]
        requires-dist = [{ name = "uv-private-pypackage", git = "https://github.com/astral-test/uv-private-pypackage" }]

        [[package]]
        name = "uv-private-pypackage"
        version = "0.1.0"
        source = { git = "https://github.com/astral-test/uv-private-pypackage#d780faf0ac91257d4d5a4f0c5a0e4509608c0071" }
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(&filters, context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(&filters, context.sync().arg("--frozen").arg("--reinstall").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + foo==0.1.0 (from file://[TEMP_DIR]/)
     + uv-private-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-private-pypackage@d780faf0ac91257d4d5a4f0c5a0e4509608c0071)
    "###);

    Ok(())
}

#[test]
fn lock_redact_index_sources() -> Result<()> {
    let context = TestContext::new("3.12").with_filtered_link_mode_warning();
    let token = decode_token(common::READ_ONLY_GITHUB_TOKEN);

    let filters: Vec<_> = [(token.as_str(), "***")]
        .into_iter()
        .chain(context.filters())
        .collect();

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "foo"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig>=2"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [[tool.uv.index]]
        name = "private"
        url = "https://public:heron@pypi-proxy.fly.dev/basic-auth/simple"

        [tool.uv.sources]
        iniconfig = { index = "private" }
        "#,
    )?;

    uv_snapshot!(&filters, context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => filters.clone(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "foo"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig", specifier = ">=2", index = "https://pypi-proxy.fly.dev/basic-auth/simple" }]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi-proxy.fly.dev/basic-auth/simple" }
        sdist = { url = "https://pypi-proxy.fly.dev/basic-auth/files/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://pypi-proxy.fly.dev/basic-auth/files/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(&filters, context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(&filters, context.sync().arg("--frozen").arg("--reinstall").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + foo==0.1.0 (from file://[TEMP_DIR]/)
     + iniconfig==2.0.0
    "###);

    Ok(())
}

/// We don't currently redact credentials from direct URLs, though.
#[test]
fn lock_redact_url_sources() -> Result<()> {
    let context = TestContext::new("3.12").with_filtered_link_mode_warning();
    let token = decode_token(common::READ_ONLY_GITHUB_TOKEN);

    let filters: Vec<_> = [(token.as_str(), "***")]
        .into_iter()
        .chain(context.filters())
        .collect();

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(r#"
        [project]
        name = "foo"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig>=2"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sources]
        iniconfig = { url = "https://public:heron@pypi-proxy.fly.dev/basic-auth/files/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl" }
        "#)?;

    uv_snapshot!(&filters, context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => filters.clone(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "foo"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig", url = "https://public:heron@pypi-proxy.fly.dev/basic-auth/files/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl" }]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { url = "https://public:heron@pypi-proxy.fly.dev/basic-auth/files/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl" }
        wheels = [
            { url = "https://public:heron@pypi-proxy.fly.dev/basic-auth/files/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374" },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(&filters, context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(&filters, context.sync().arg("--frozen").arg("--reinstall").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + foo==0.1.0 (from file://[TEMP_DIR]/)
     + iniconfig==2.0.0 (from https://public:heron@pypi-proxy.fly.dev/basic-auth/files/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl)
    "###);

    Ok(())
}

/// Pass credentials for a named index via environment variables.
#[test]
fn lock_env_credentials() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "foo"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [[tool.uv.index]]
        name = "internal-proxy"
        url = "https://pypi-proxy.fly.dev/basic-auth/simple"
        default = true
        "#,
    )?;

    // Without credentials, the resolution should fail.
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because iniconfig was not found in the package registry and your project depends on iniconfig, we can conclude that your project's requirements are unsatisfiable.

          hint: An index URL (https://pypi-proxy.fly.dev/basic-auth/simple) could not be queried due to a lack of valid authentication credentials (401 Unauthorized).
    "###);

    // Provide credentials via environment variables.
    uv_snapshot!(context.filters(), context.lock()
        .env(EnvVars::index_username("INTERNAL_PROXY"), "public")
        .env(EnvVars::index_password("INTERNAL_PROXY"), "heron"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock")).unwrap();

    // The lockfile shout omit the credentials.
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "foo"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig" }]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi-proxy.fly.dev/basic-auth/simple" }
        sdist = { url = "https://pypi-proxy.fly.dev/basic-auth/files/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://pypi-proxy.fly.dev/basic-auth/files/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]
        "###
        );
    });

    Ok(())
}

/// Resolve against an index that uses relative links.
#[test]
fn lock_relative_index() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "foo"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv]
        index-url = "https://pypi-proxy.fly.dev/relative/simple"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "foo"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig" }]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi-proxy.fly.dev/relative/simple" }
        sdist = { url = "https://pypi-proxy.fly.dev/files/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://pypi-proxy.fly.dev/files/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + foo==0.1.0 (from file://[TEMP_DIR]/)
     + iniconfig==2.0.0
    "###);

    Ok(())
}

/// Lock a package that's excluded from the parent workspace, but depends on that parent.
#[test]
fn lock_no_sources() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv]
        dev-dependencies = ["typing-extensions>4"]

        [tool.uv.sources]
        anyio = { path = "./anyio" }
        "#,
    )?;

    let anyio = context.temp_dir.child("anyio");
    fs_err::create_dir_all(&anyio)?;

    let pyproject_toml = anyio.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "anyio"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    // Lock the root package with `tool.uv.sources` enabled.
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "0.1.0"
        source = { directory = "anyio" }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig" }]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio" },
        ]

        [package.dev-dependencies]
        dev = [
            { name = "typing-extensions" },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio", directory = "anyio" }]

        [package.metadata.requires-dev]
        dev = [{ name = "typing-extensions", specifier = ">4" }]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    // Lock the root package with `tool.uv.sources` disabled.
    uv_snapshot!(context.filters(), context.lock().arg("--no-sources"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Updated anyio v0.1.0 -> v4.3.0
    Added idna v3.6
    Removed iniconfig v2.0.0
    Added sniffio v1.3.1
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
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
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio" },
        ]

        [package.dev-dependencies]
        dev = [
            { name = "typing-extensions" },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio" }]

        [package.metadata.requires-dev]
        dev = [{ name = "typing-extensions", specifier = ">4" }]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--no-sources").arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###);

    Ok(())
}

/// Lock a project that has an existing lockfile with a deprecated schema.
#[test]
fn lock_migrate() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    // Write a lockfile with a deprecated schema.
    let lock = context.temp_dir.child("uv.lock");
    lock.write_str(
        r#"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[distribution-term-we-dont-know]]
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[distribution-term-we-dont-know]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[distribution-term-we-dont-know]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
        { name = "anyio" },
        ]

        [[distribution-term-we-dont-know]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "#
    )?;

    uv_snapshot!(context.filters(), context.lock().current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Added anyio v4.3.0
    Added idna v3.6
    Added project v0.1.0
    Added sniffio v1.3.1
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
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
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio" }]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    Ok(())
}

/// Upgrade a specific package with `--upgrade-package`.
#[test]
fn lock_upgrade_package() -> Result<()> {
    let context = TestContext::new("3.12");

    // Constrain `anyio` and `idna.`
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio<=2", "idna<=3"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    // Lock the root package.
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/fe/dc/daeadb9b34093d3968afcc93946ee567cd6d2b402a96c608cb160f74d737/anyio-2.0.0.tar.gz", hash = "sha256:ceca4669ffa3f02bf20ef3d6c2a0c323b16cdc71d1ce0b0bc03c6f1f36054826", size = 91291 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/8a/19/10fe682e962efd1610aa41376399fc3f3e002425449b02d0fb04749bb712/anyio-2.0.0-py3-none-any.whl", hash = "sha256:0b8375c8fc665236cb4d143ea13e849eb9e074d727b1b5c27d88aba44ca8c547", size = 62675 },
        ]

        [[package]]
        name = "idna"
        version = "3.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/2f/2e/bfe821bd26194fb474e0932df8ed82e24bd312ba628a8644d93c5a28b5d4/idna-3.0.tar.gz", hash = "sha256:c9a26e10e5558412384fac891eefb41957831d31be55f1e2c98ed97a70abb969", size = 180786 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/0f/6b/3a878f15ef3324754bf4780f8f047d692d9860be894ff8fb3135cef8bed8/idna-3.0-py2.py3-none-any.whl", hash = "sha256:320229aadbdfc597bc28876748cc0c9d04d476e0fe6caacaaddea146365d9f63", size = 58618 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio" },
            { name = "idna" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "anyio", specifier = "<=2" },
            { name = "idna", specifier = "<=3" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    // Remove the constraints.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio", "idna"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    // Upgrade `anyio`, but nothing else.
    uv_snapshot!(context.filters(), context.lock().arg("--upgrade-package").arg("anyio"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Updated anyio v2.0.0 -> v4.3.0
    "###);

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
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
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "idna"
        version = "3.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/2f/2e/bfe821bd26194fb474e0932df8ed82e24bd312ba628a8644d93c5a28b5d4/idna-3.0.tar.gz", hash = "sha256:c9a26e10e5558412384fac891eefb41957831d31be55f1e2c98ed97a70abb969", size = 180786 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/0f/6b/3a878f15ef3324754bf4780f8f047d692d9860be894ff8fb3135cef8bed8/idna-3.0-py2.py3-none-any.whl", hash = "sha256:320229aadbdfc597bc28876748cc0c9d04d476e0fe6caacaaddea146365d9f63", size = 58618 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio" },
            { name = "idna" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "anyio" },
            { name = "idna" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    // Upgrade everything.
    uv_snapshot!(context.filters(), context.lock().arg("--upgrade"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Updated idna v3.0 -> v3.6
    "###);

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
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
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio" },
            { name = "idna" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "anyio" },
            { name = "idna" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    Ok(())
}

/// Check that we discard the fork marker from the lockfile when using `--upgrade`.
#[test]
fn lock_upgrade_drop_fork_markers() -> Result<()> {
    let context = TestContext::new("3.12");

    let requirements = r#"[project]
    name = "forking"
    version = "0.1.0"
    requires-python = ">=3.12"
    dependencies = ["fork-upgrade-foo==1"]

    [build-system]
        requires = ["flit_core>=3.8,<4"]
        build-backend = "flit_core.buildapi"
    "#;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(requirements)?;
    context
        .lock()
        .arg("--index-url")
        .arg(packse_index_url())
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .assert()
        .success();
    let lock = context.read("uv.lock");
    assert!(lock.contains("resolution-markers"));

    // Remove the bound and lock with `--upgrade`.
    pyproject_toml.write_str(&requirements.replace("fork-upgrade-foo==1", "fork-upgrade-foo"))?;
    context
        .lock()
        .arg("--index-url")
        .arg(packse_index_url())
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .arg("--upgrade")
        .assert()
        .success();
    let lock = context.read("uv.lock");
    assert!(!lock.contains("resolution-markers"));
    Ok(())
}

/// Warn when there are missing bounds on transitive dependencies with `--resolution lowest`.
#[test]
fn lock_warn_missing_transitive_lower_bounds() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "foo"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["pytest>8"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().arg("--resolution").arg("lowest"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    warning: The transitive dependency `colorama` is unpinned. Consider setting a lower bound with a constraint when using `--resolution lowest` to avoid using outdated versions.
    warning: The transitive dependency `packaging` is unpinned. Consider setting a lower bound with a constraint when using `--resolution lowest` to avoid using outdated versions.
    warning: The transitive dependency `iniconfig` is unpinned. Consider setting a lower bound with a constraint when using `--resolution lowest` to avoid using outdated versions.
    "###);

    Ok(())
}

/// Lock a local wheel via `--find-links`.
#[test]
fn lock_find_links_local_wheel() -> Result<()> {
    let context = TestContext::new("3.12");

    // Populate the `--find-links` entries.
    fs_err::create_dir_all(context.temp_dir.join("links"))?;

    for entry in fs_err::read_dir(context.workspace_root.join("scripts/links"))? {
        let entry = entry?;
        let path = entry.path();
        if path
            .file_name()
            .and_then(|file_name| file_name.to_str())
            .is_some_and(|file_name| file_name.starts_with("tqdm-"))
        {
            let dest = context
                .temp_dir
                .join("links")
                .join(path.file_name().unwrap());
            fs_err::copy(&path, &dest)?;
        }
    }

    let workspace = context.temp_dir.child("workspace");

    let pyproject_toml = workspace.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc! { r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["tqdm==1000.0.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv]
        find-links = ["{}"]
        "#,
        context.temp_dir.join("links/").portable_display(),
    })?;

    uv_snapshot!(context.filters(), context.lock().current_dir(&workspace), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(workspace.join("uv.lock")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "tqdm" },
        ]

        [package.metadata]
        requires-dist = [{ name = "tqdm", specifier = "==1000.0.0" }]

        [[package]]
        name = "tqdm"
        version = "1000.0.0"
        source = { registry = "../links" }
        wheels = [
            { path = "tqdm-1000.0.0-py3-none-any.whl" },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").current_dir(&workspace), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 2 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").current_dir(&workspace), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Prepared 1 package in [TIME]
    Installed 2 packages in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/workspace)
     + tqdm==1000.0.0
    "###);

    Ok(())
}

/// Lock a local source distribution via `--find-links`.
#[test]
fn lock_find_links_local_sdist() -> Result<()> {
    let context = TestContext::new("3.12");

    // Populate the `--find-links` entries.
    fs_err::create_dir_all(context.temp_dir.join("links"))?;

    for entry in fs_err::read_dir(context.workspace_root.join("scripts/links"))? {
        let entry = entry?;
        let path = entry.path();
        if path
            .file_name()
            .and_then(|file_name| file_name.to_str())
            .is_some_and(|file_name| file_name.starts_with("tqdm-"))
        {
            let dest = context
                .temp_dir
                .join("links")
                .join(path.file_name().unwrap());
            fs_err::copy(&path, &dest)?;
        }
    }

    let workspace = context.temp_dir.child("workspace");

    let pyproject_toml = workspace.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc! { r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["tqdm==999.0.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv]
        find-links = ["{}"]
        "#,
        context.temp_dir.join("links/").portable_display(),
    })?;

    uv_snapshot!(context.filters(), context.lock().current_dir(&workspace), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(workspace.join("uv.lock")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "tqdm" },
        ]

        [package.metadata]
        requires-dist = [{ name = "tqdm", specifier = "==999.0.0" }]

        [[package]]
        name = "tqdm"
        version = "999.0.0"
        source = { registry = "../links" }
        sdist = { path = "tqdm-999.0.0.tar.gz" }
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").current_dir(&workspace), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 2 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").current_dir(&workspace), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/workspace)
     + tqdm==999.0.0
    "###);

    Ok(())
}

/// Lock a wheel over HTTP via `--find-links`.
#[test]
fn lock_find_links_http_wheel() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc! { r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["packaging==23.2"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv]
        find-links = ["{}"]
        no-build-package = ["packaging"]
        "#,
        build_vendor_links_url()
    })?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "packaging"
        version = "23.2"
        source = { registry = "https://raw.githubusercontent.com/astral-sh/packse/PACKSE_VERSION/vendor/links.html" }
        sdist = { url = "https://raw.githubusercontent.com/astral-sh/packse/PACKSE_VERSION/vendor/build/packaging-23.2.tar.gz" }
        wheels = [
            { url = "https://raw.githubusercontent.com/astral-sh/packse/PACKSE_VERSION/vendor/build/packaging-23.2-py3-none-any.whl" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "packaging" },
        ]

        [package.metadata]
        requires-dist = [{ name = "packaging", specifier = "==23.2" }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 2 packages in [TIME]
     + packaging==23.2
     + project==0.1.0 (from file://[TEMP_DIR]/)
    "###);

    Ok(())
}

/// Lock a source distribution over HTTP via `--find-links`.
#[test]
fn lock_find_links_http_sdist() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc! { r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["packaging==23.2"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv]
        find-links = ["{}"]
        no-binary-package = ["packaging"]
        "#,
        build_vendor_links_url()
    })?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "packaging"
        version = "23.2"
        source = { registry = "https://raw.githubusercontent.com/astral-sh/packse/PACKSE_VERSION/vendor/links.html" }
        sdist = { url = "https://raw.githubusercontent.com/astral-sh/packse/PACKSE_VERSION/vendor/build/packaging-23.2.tar.gz" }
        wheels = [
            { url = "https://raw.githubusercontent.com/astral-sh/packse/PACKSE_VERSION/vendor/build/packaging-23.2-py3-none-any.whl" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "packaging" },
        ]

        [package.metadata]
        requires-dist = [{ name = "packaging", specifier = "==23.2" }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + packaging==23.2
     + project==0.1.0 (from file://[TEMP_DIR]/)
    "###);

    Ok(())
}

/// Lock against a local directory laid out as a PEP 503-compatible index.
#[test]
fn lock_local_index() -> Result<()> {
    let context = TestContext::new("3.12");

    let root = context.temp_dir.child("simple-html");
    fs_err::create_dir_all(&root)?;

    let tqdm = root.child("tqdm");
    fs_err::create_dir_all(&tqdm)?;

    let wheel = tqdm.child("tqdm-1000.0.0-py3-none-any.whl");
    fs_err::copy(
        context
            .workspace_root
            .join("scripts/links/tqdm-1000.0.0-py3-none-any.whl"),
        &wheel,
    )?;

    let index = tqdm.child("index.html");
    index.write_str(&formatdoc! {r#"
        <!DOCTYPE html>
        <html>
          <head>
            <meta name="pypi:repository-version" content="1.1" />
          </head>
          <body>
            <h1>Links for tqdm</h1>
            <a
              href="{}"
              data-requires-python=">=3.8"
            >
              tqdm-1000.0.0-py3-none-any.whl
            </a>
          </body>
        </html>
    "#, Url::from_file_path(wheel).unwrap().as_str()})?;

    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc! { r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["tqdm"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv]
        extra-index-url = ["{}"]
        "#,
        Url::from_file_path(&root).unwrap().as_str()
    })?;

    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    let index = Url::from_file_path(&root).unwrap().to_string();
    let filters = [(index.as_str(), "file://[TMP]")]
        .into_iter()
        .chain(context.filters())
        .collect::<Vec<_>>();

    insta::with_settings!({
        filters => filters,
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "tqdm" },
        ]

        [package.metadata]
        requires-dist = [{ name = "tqdm" }]

        [[package]]
        name = "tqdm"
        version = "1000.0.0"
        source = { registry = "../../[TMP]/simple-html" }
        wheels = [
            { path = "tqdm/tqdm-1000.0.0-py3-none-any.whl" },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 2 packages in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + tqdm==1000.0.0
    "###);

    Ok(())
}

/// Lock a direct URL requirement that uses `tool.uv.sources` internally (specifically, the root
/// package `workspace` depends on `anyio`, and declares a source pointing to a local stub of
/// `anyio`).
///
/// When resolving, we should ignore the `tool.uv.sources` and instead pull in `anyio` from PyPI.
#[test]
fn lock_sources_url() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["workspace @ https://github.com/user-attachments/files/16592193/workspace.zip"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
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
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "workspace" },
        ]

        [package.metadata]
        requires-dist = [{ name = "workspace", url = "https://github.com/user-attachments/files/16592193/workspace.zip" }]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]

        [[package]]
        name = "workspace"
        version = "0.1.0"
        source = { url = "https://github.com/user-attachments/files/16592193/workspace.zip" }
        dependencies = [
            { name = "anyio" },
        ]
        sdist = { url = "https://github.com/user-attachments/files/16592193/workspace.zip", hash = "sha256:ba690a925dc3d1b53e0675201c9ec26ab59eeec72ab271562f53297bf1817263" }

        [package.metadata]
        requires-dist = [{ name = "anyio" }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.1
     + workspace==0.1.0 (from https://github.com/user-attachments/files/16592193/workspace.zip)
    "###);

    Ok(())
}

/// Lock a local archive requirement that uses `tool.uv.sources` internally (specifically, the root
/// package `workspace` depends on `anyio`, and declares a source pointing to a local stub of
/// `anyio`).
///
/// When resolving, we should ignore the `tool.uv.sources` and instead pull in `anyio` from PyPI.
#[test]
fn lock_sources_archive() -> Result<()> {
    let context = TestContext::new("3.12");

    // Download the source.
    let workspace_archive = context.temp_dir.child("workspace.zip");
    download_to_disk(
        "https://github.com/user-attachments/files/16592193/workspace.zip",
        &workspace_archive,
    );

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc! {
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["workspace @ {}"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
        Url::from_file_path(&workspace_archive).unwrap(),
    })?;

    uv_snapshot!( context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    // insta::with_settings!({
    //     filters => context.filters(),
    // }, {
    assert_snapshot!(
        lock, @r###"
        version = 1
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
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "workspace" },
        ]

        [package.metadata]
        requires-dist = [{ name = "workspace", path = "workspace.zip" }]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]

        [[package]]
        name = "workspace"
        version = "0.1.0"
        source = { path = "workspace.zip" }
        dependencies = [
            { name = "anyio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio" }]
        "###
    );
    // });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.1
     + workspace==0.1.0 (from file://[TEMP_DIR]/workspace.zip)
    "###);

    Ok(())
}

/// Lock a local archive requirement that uses `tool.uv.sources` internally (specifically, the root
/// package `workspace` depends on `anyio`, and declares a source pointing to a local stub of
/// `anyio`).
///
/// When resolving, we should respect the `tool.uv.sources` and pull in the stub `anyio`.
#[test]
fn lock_sources_source_tree() -> Result<()> {
    let context = TestContext::new("3.12");

    // Download the source.
    let workspace_archive = context.temp_dir.child("workspace.zip");
    download_to_disk(
        "https://github.com/user-attachments/files/16592193/workspace.zip",
        &workspace_archive,
    );

    // Unzip the file.
    let file = fs_err::File::open(&*workspace_archive)?;
    let mut archive = zip::ZipArchive::new(BufReader::new(file))?;
    archive.extract(&context.temp_dir)?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc! {
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["workspace @ file://{}"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"

        [tool.hatch.metadata]
        allow-direct-references = true

        [tool.hatch.build.targets.wheel]
        packages = ["src/project"]
        "#,
        context.temp_dir.child("workspace").portable_display()
    })?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "0.1.0"
        source = { editable = "workspace/anyio" }

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "workspace" },
        ]

        [package.metadata]
        requires-dist = [{ name = "workspace", directory = "workspace" }]

        [[package]]
        name = "workspace"
        version = "0.1.0"
        source = { directory = "workspace" }
        dependencies = [
            { name = "anyio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio", editable = "workspace/anyio" }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==0.1.0 (from file://[TEMP_DIR]/workspace/anyio)
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + workspace==0.1.0 (from file://[TEMP_DIR]/workspace)
    "###);

    Ok(())
}

/// Lock a project in which a given dependency is requested from two different members, once as
/// editable, and once as non-editable.
#[test]
fn lock_editable() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create the workspace root.
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "workspace"
        version = "0.1.0"
        dependencies = ["library"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sources]
        library = { path = "./library" }

        [tool.uv.workspace]
        members = ["packages/*"]
    "#})?;
    context.temp_dir.child("src/__init__.py").touch()?;

    // Create a package.
    let leaf = context.temp_dir.child("packages").child("leaf");
    leaf.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "leaf"
        version = "0.1.0"
        dependencies = ["library"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sources]
        library = { path = "../../library" }
    "#})?;
    leaf.child("src/__init__.py").touch()?;

    // Create a peripheral library.
    let library = context.temp_dir.child("library");
    library.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "library"
        version = "0.1.0"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
    "#})?;
    library.child("src/__init__.py").touch()?;

    // First, lock without marking the dependency as editable from either member.
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        members = [
            "leaf",
            "workspace",
        ]

        [[package]]
        name = "leaf"
        version = "0.1.0"
        source = { editable = "packages/leaf" }
        dependencies = [
            { name = "library" },
        ]

        [package.metadata]
        requires-dist = [{ name = "library", directory = "library" }]

        [[package]]
        name = "library"
        version = "0.1.0"
        source = { directory = "library" }

        [[package]]
        name = "workspace"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "library" },
        ]

        [package.metadata]
        requires-dist = [{ name = "library", directory = "library" }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    // Next, mark the dependency as editable from one member.
    leaf.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "leaf"
        version = "0.1.0"
        dependencies = ["library"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sources]
        library = { path = "../../library", editable = true }
    "#})?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        members = [
            "leaf",
            "workspace",
        ]

        [[package]]
        name = "leaf"
        version = "0.1.0"
        source = { editable = "packages/leaf" }
        dependencies = [
            { name = "library" },
        ]

        [package.metadata]
        requires-dist = [{ name = "library", editable = "library" }]

        [[package]]
        name = "library"
        version = "0.1.0"
        source = { editable = "library" }

        [[package]]
        name = "workspace"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "library" },
        ]

        [package.metadata]
        requires-dist = [{ name = "library", directory = "library" }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + library==0.1.0 (from file://[TEMP_DIR]/library)
     + workspace==0.1.0 (from file://[TEMP_DIR]/)
    "###);

    Ok(())
}

/// Lock a project in which a given dependency is requested from two different members, once as
/// editable, and once as non-editable.
#[test]
fn lock_mixed_extras() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create a workspace (`workspace1`) with a dependency on another workspace (`workspace2`).
    let workspace1 = context.temp_dir.child("workspace1");
    workspace1.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "workspace1"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["leaf1", "workspace2"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [project.optional-dependencies]
        async = ["typing-extensions>=4"]

        [tool.uv.sources]
        leaf1 = { workspace = true }
        workspace2 = { path = "../workspace2" }

        [tool.uv.workspace]
        members = ["packages/*"]
    "#})?;
    workspace1.child("src/__init__.py").touch()?;

    let leaf1 = workspace1.child("packages").child("leaf1");
    leaf1.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "leaf1"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.optional-dependencies]
        async = ["iniconfig>=2"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
    "#})?;
    leaf1.child("src/__init__.py").touch()?;

    // Create a second workspace (`workspace2`) with an extra of the same name.
    let workspace2 = context.temp_dir.child("workspace2");
    workspace2.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "workspace2"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["leaf2"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sources]
        leaf2 = { workspace = true }

        [tool.uv.workspace]
        members = ["packages/*"]
    "#})?;
    workspace2.child("src/__init__.py").touch()?;

    let leaf2 = workspace2.child("packages").child("leaf2");
    leaf2.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "leaf2"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.optional-dependencies]
        async = ["packaging>=24"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
    "#})?;
    leaf2.child("src/__init__.py").touch()?;

    // Lock the first workspace.
    uv_snapshot!(context.filters(), context.lock().current_dir(&workspace1), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 6 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(workspace1.join("uv.lock")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        members = [
            "leaf1",
            "workspace1",
        ]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "leaf1"
        version = "0.1.0"
        source = { editable = "packages/leaf1" }

        [package.optional-dependencies]
        async = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig", marker = "extra == 'async'", specifier = ">=2" }]

        [[package]]
        name = "leaf2"
        version = "0.1.0"
        source = { editable = "../workspace2/packages/leaf2" }

        [package.metadata]
        requires-dist = [{ name = "packaging", marker = "extra == 'async'", specifier = ">=24" }]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 },
        ]

        [[package]]
        name = "workspace1"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "leaf1" },
            { name = "workspace2" },
        ]

        [package.optional-dependencies]
        async = [
            { name = "typing-extensions" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "leaf1", editable = "packages/leaf1" },
            { name = "typing-extensions", marker = "extra == 'async'", specifier = ">=4" },
            { name = "workspace2", directory = "../workspace2" },
        ]

        [[package]]
        name = "workspace2"
        version = "0.1.0"
        source = { directory = "../workspace2" }
        dependencies = [
            { name = "leaf2" },
        ]

        [package.metadata]
        requires-dist = [{ name = "leaf2", editable = "../workspace2/packages/leaf2" }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").current_dir(&workspace1), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 6 packages in [TIME]
    "###);

    // Install from the lockfile. This should include the first-party packages, but no third-party
    // packages, since they all rely on extras.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").current_dir(&workspace1), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + leaf1==0.1.0 (from file://[TEMP_DIR]/workspace1/packages/leaf1)
     + leaf2==0.1.0 (from file://[TEMP_DIR]/workspace2/packages/leaf2)
     + workspace1==0.1.0 (from file://[TEMP_DIR]/workspace1)
     + workspace2==0.1.0 (from file://[TEMP_DIR]/workspace2)
    "###);

    // Install from the lockfile with the `async` extra. This should include `typing-extensions`,
    // but not `iniconfig` or `packaging`, since we're installing the root package, whereas
    // `iniconfig` is an extra on another package, and `packaging` is an extra in another workspace.
    uv_snapshot!(context.filters(), context.sync().arg("--extra").arg("async").arg("--frozen").current_dir(&workspace1), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.10.0
    "###);

    Ok(())
}

/// Lock a project in which a given dependency is requested from two different members, once as
/// editable, and once as non-editable.
#[test]
fn lock_transitive_extra() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create a workspace (`workspace1`) with a dependency on another workspace (`workspace2`).
    let workspace = context.temp_dir.child("workspace");
    workspace.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "workspace"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["leaf"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [project.optional-dependencies]
        async = ["typing-extensions>=4", "leaf[async]"]

        [tool.uv.sources]
        leaf = { workspace = true }
        workspace2 = { path = "../workspace2" }

        [tool.uv.workspace]
        members = ["packages/*"]
    "#})?;
    workspace.child("src/__init__.py").touch()?;

    let leaf = workspace.child("packages").child("leaf");
    leaf.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "leaf"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.optional-dependencies]
        async = ["iniconfig>=2"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
    "#})?;
    leaf.child("src/__init__.py").touch()?;

    // Lock the workspace.
    uv_snapshot!(context.filters(), context.lock().current_dir(&workspace), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 4 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(workspace.join("uv.lock")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        members = [
            "leaf",
            "workspace",
        ]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "leaf"
        version = "0.1.0"
        source = { editable = "packages/leaf" }

        [package.optional-dependencies]
        async = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig", marker = "extra == 'async'", specifier = ">=2" }]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 },
        ]

        [[package]]
        name = "workspace"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "leaf" },
        ]

        [package.optional-dependencies]
        async = [
            { name = "leaf", extra = ["async"] },
            { name = "typing-extensions" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "leaf", editable = "packages/leaf" },
            { name = "leaf", extras = ["async"], marker = "extra == 'async'", editable = "packages/leaf" },
            { name = "typing-extensions", marker = "extra == 'async'", specifier = ">=4" },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").current_dir(&workspace), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 4 packages in [TIME]
    "###);

    // Install from the lockfile. This should include the first-party packages, but no third-party
    // packages, since they all rely on extras.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").current_dir(&workspace), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Creating virtual environment at: .venv
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + leaf==0.1.0 (from file://[TEMP_DIR]/workspace/packages/leaf)
     + workspace==0.1.0 (from file://[TEMP_DIR]/workspace)
    "###);

    // Install from the lockfile with the `async` extra. This should include `typing-extensions`
    // and `iniconfig`.
    uv_snapshot!(context.filters(), context.sync().arg("--extra").arg("async").arg("--frozen").current_dir(&workspace), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     + typing-extensions==4.10.0
    "###);

    Ok(())
}

/// If a source is provided via `tool.uv.sources` _and_ a URL is provided in `project.dependencies`,
/// we accept the source in `tool.uv.sources`, unless `--no-sources` is provided.
#[test]
fn lock_mismatched_sources() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@0.0.2",
        ]

        [tool.uv.sources]
        uv-public-pypackage = { git = "https://github.com/astral-test/uv-public-pypackage", tag = "0.0.1" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
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
        "###
        );
    });

    // If we run with `--no-sources`, we should use the URL provided in `project.dependencies`.
    uv_snapshot!(context.filters(), context.lock().arg("--no-sources"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
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
        requires-dist = [{ name = "uv-public-pypackage", git = "https://github.com/astral-test/uv-public-pypackage?rev=0.0.2" }]

        [[package]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = { git = "https://github.com/astral-test/uv-public-pypackage?rev=0.0.2#b270df1a2fb5d012294e9aaf05e7e0bab1e6a389" }
        "###
        );
    });

    Ok(())
}

/// If a source is provided via `tool.uv.sources`, we don't enforce that the declared dependency
/// version is satisfied.
///
/// See: <https://github.com/astral-sh/uv/issues/4604>
#[test]
fn lock_mismatched_versions() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["uv-public-pypackage==0.0.2"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sources]
        uv-public-pypackage = { git = "https://github.com/astral-test/uv-public-pypackage", tag = "0.0.1" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "uv-public-pypackage" },
        ]

        [package.metadata]
        requires-dist = [{ name = "uv-public-pypackage", git = "https://github.com/astral-test/uv-public-pypackage?tag=0.0.1" }]

        [[package]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = { git = "https://github.com/astral-test/uv-public-pypackage?tag=0.0.1#0dacfd662c64cb4ceb16e6cf65a157a8b715b979" }
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@0dacfd662c64cb4ceb16e6cf65a157a8b715b979)
    "###);

    Ok(())
}

/// This checks that overlapping marker expressions with disjoint
/// version constraints fails to resolve.
#[test]
fn unconditional_overlapping_marker_disjoint_version_constraints() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.11"
        dependencies = [
            "datasets < 2.19",
            "datasets >= 2.19 ; python_version > '3.10'"
        ]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only datasets<2.19 is available and your project depends on datasets>=2.19, we can conclude that your project's requirements are unsatisfiable.
    "###);

    Ok(())
}

/// Change indexes between locking operations.
#[test]
fn lock_change_index() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().arg("--index-url").arg("https://public:heron@pypi-proxy.fly.dev/basic-auth/simple"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi-proxy.fly.dev/basic-auth/simple" }
        sdist = { url = "https://pypi-proxy.fly.dev/basic-auth/files/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://pypi-proxy.fly.dev/basic-auth/files/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig" }]
        "###
        );
    });

    // Re-run against PyPI.
    uv_snapshot!(context.filters(), context.lock().arg("--index-url").arg("https://pypi.org/simple"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig" }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    Ok(())
}

/// Lock a `pyproject.toml`, add a remove a workspace member, and ensure that the lockfile is
/// updated on the next run.
#[test]
fn lock_remove_member() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create a workspace member.
    let leaf = context.temp_dir.child("leaf");
    leaf.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "leaf"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio>3"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["leaf"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.workspace]
        members = ["leaf"]

        [tool.uv.sources]
        leaf = { workspace = true }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        members = [
            "leaf",
            "project",
        ]

        [[package]]
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "leaf"
        version = "0.1.0"
        source = { editable = "leaf" }
        dependencies = [
            { name = "anyio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio", specifier = ">3" }]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "leaf" },
        ]

        [package.metadata]
        requires-dist = [{ name = "leaf", editable = "leaf" }]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###);

    // Remove the member.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
        "#,
    )?;

    // Re-run with `--locked`. This should fail.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided. To update the lockfile, run `uv lock`.
    "###);

    // Re-run without `--locked`.
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Removed anyio v4.3.0
    Removed idna v3.6
    Removed leaf v0.1.0
    Removed sniffio v1.3.1
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        "###
        );
    });

    Ok(())
}

/// Lock a `pyproject.toml`, add a workspace member that _isn't_ a dependency of the root, and
/// ensure that the lockfile is updated on the next run.
///
/// This test would fail if we didn't write the list of workspace members to the lockfile, since
/// we wouldn't be able to determine that a new member was added.
#[test]
fn lock_add_member() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create a workspace, but don't add the member.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.workspace]
        members = []
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    "###);

    // Create a workspace member.
    let leaf = context.temp_dir.child("leaf");
    leaf.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "leaf"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio>3"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    // Add the member to the workspace, but not as a dependency of the root.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.workspace]
        members = ["leaf"]
        "#,
    )?;

    // Re-run with `--locked`. This should fail.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided. To update the lockfile, run `uv lock`.
    "###);

    // Re-run with `--offline`. This should also fail, during the resolve phase.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because anyio was not found in the cache and leaf depends on anyio>3, we can conclude that leaf's requirements are unsatisfiable.
          And because your workspace requires leaf, we can conclude that your workspace's requirements are unsatisfiable.

          hint: Packages were unavailable because the network was disabled. When the network is disabled, registry packages may only be read from the cache.
    "###);

    // Re-run without `--locked`.
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Added anyio v4.3.0
    Added idna v3.6
    Added leaf v0.1.0
    Added sniffio v1.3.1
    "###);

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        members = [
            "leaf",
            "project",
        ]

        [[package]]
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "leaf"
        version = "0.1.0"
        source = { editable = "leaf" }
        dependencies = [
            { name = "anyio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio", specifier = ">3" }]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    Ok(())
}

/// Lock a `pyproject.toml`, then add a dependency that's already included in the resolution.
/// In theory, we shouldn't need to re-resolve, but based on our current strategy, we don't accept
/// the existing lockfile.
#[test]
fn lock_redundant_add_member() -> Result<()> {
    let context = TestContext::new("3.12");

    // Lock `anyio`.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.workspace]
        members = []
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
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
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio" }]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    // Add a dependency that's already included in the lockfile.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio", "idna"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.workspace]
        members = []
        "#,
    )?;

    // Re-run with `--locked`. This will fail, though in theory it could succeed, since the current
    // _resolution_ satisfies the requirements, even if the inputs are not identical
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided. To update the lockfile, run `uv lock`.
    "###);

    // Re-run without `--locked`.
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
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
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio" },
            { name = "idna" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "anyio" },
            { name = "idna" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    Ok(())
}

/// Lock a `pyproject.toml`, add a new constraint, and ensure that the lockfile is updated on the
/// next run.
#[test]
fn lock_new_constraints() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
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
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio" }]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    // Add a constraint.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv]
        constraint-dependencies = [
            "anyio < 4.3",
        ]
        "#,
    )?;

    // Re-run with `--locked`. This should fail.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided. To update the lockfile, run `uv lock`.
    "###);

    // Re-run without `--locked`.
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Updated anyio v4.3.0 -> v4.2.0
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        constraints = [{ name = "anyio", specifier = "<4.3" }]

        [[package]]
        name = "anyio"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/2d/b8/7333d87d5f03247215d86a86362fd3e324111788c6cdd8d2e6196a6ba833/anyio-4.2.0.tar.gz", hash = "sha256:e1875bb4b4e2de1669f4bc7869b6d3f54231cdced71605e6e64c9be77e3be50f", size = 158770 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/bf/cd/d6d9bb1dadf73e7af02d18225cbd2c93f8552e13130484f1c8dcfece292b/anyio-4.2.0-py3-none-any.whl", hash = "sha256:745843b39e829e108e518c489b31dc757de7d2131d53fac32bd8df268227bfee", size = 85481 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio" }]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    Ok(())
}

/// Lock a `pyproject.toml`, add a new constraint, and ensure that the lockfile is updated on the
/// next run.
#[test]
fn lock_remove_member_non_project() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create a virtual workspace root.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [tool.uv.workspace]
        members = ["leaf"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    // Create a workspace member.
    let leaf = context.temp_dir.child("leaf");
    leaf.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "leaf"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio>3"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        members = [
            "leaf",
        ]

        [[package]]
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "leaf"
        version = "0.1.0"
        source = { editable = "leaf" }
        dependencies = [
            { name = "anyio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio", specifier = ">3" }]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    // Remove the member.
    pyproject_toml.write_str(
        r"
        [tool.uv.workspace]
        members = []
        ",
    )?;

    // Re-run with `--locked`. This should fail.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: No `requires-python` value found in the workspace. Defaulting to `>=3.12`.
    Resolved in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided. To update the lockfile, run `uv lock`.
    "###);

    // Re-run without `--locked`.
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: No `requires-python` value found in the workspace. Defaulting to `>=3.12`.
    Resolved in [TIME]
    Removed anyio v4.3.0
    Removed idna v3.6
    Removed leaf v0.1.0
    Removed sniffio v1.3.1
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "###
        );
    });

    Ok(())
}

/// Lock a `pyproject.toml`, then rename the project, and ensure that the lockfile is updated on
/// the next run.
#[test]
fn lock_rename_project() -> Result<()> {
    let context = TestContext::new("3.12");

    // Create a workspace, but don't add the member.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig" }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Rename the project.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "renamed"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    // Re-run with `--locked`. This should fail.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided. To update the lockfile, run `uv lock`.
    "###);

    // Re-run without `--locked`.
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Removed project v0.1.0
    Added renamed v0.1.0
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "renamed"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig" }]
        "###
        );
    });

    Ok(())
}

/// Test backwards compatibility for `[package.metadata]`.
#[test]
fn lock_missing_metadata() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    let lock = context.temp_dir.child("uv.lock");

    // Write a lockfile with a missing `[package.metadata]` section.
    lock.write_str(r#"
        version = 1
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
        sdist = { url = "https://files.pythonhosted.org/packages/c6/b3/fefbf7e78ab3b805dec67d698dc18dd505af7a18a8dd08868c9b4fa736b5/anyio-3.7.0.tar.gz", hash = "sha256:275d9973793619a5374e1c89a4f4ad3f4b0a5510a2b5b939444bee8f4c4d37ce", size = 142737 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/fe/7ce1926952c8a403b35029e194555558514b365ad77d75125f521a2bec62/anyio-3.7.0-py3-none-any.whl", hash = "sha256:eddca883c4175f14df8aedce21054bfca3adb70ffe76a9f607aef9d7fa2ea7f0", size = 80873 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
    "#)?;

    // Re-locking should add `[package.metadata]`.
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
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
        sdist = { url = "https://files.pythonhosted.org/packages/c6/b3/fefbf7e78ab3b805dec67d698dc18dd505af7a18a8dd08868c9b4fa736b5/anyio-3.7.0.tar.gz", hash = "sha256:275d9973793619a5374e1c89a4f4ad3f4b0a5510a2b5b939444bee8f4c4d37ce", size = 142737 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/fe/7ce1926952c8a403b35029e194555558514b365ad77d75125f521a2bec62/anyio-3.7.0-py3-none-any.whl", hash = "sha256:eddca883c4175f14df8aedce21054bfca3adb70ffe76a9f607aef9d7fa2ea7f0", size = 80873 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio", specifier = "==3.7.0" }]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    Ok(())
}

/// Test backwards compatibility for `package.dependency-groups`. `package.dev-dependencies` was
/// accidentally renamed to `package.dependency-groups` in v0.4.27, which is technically out of
/// compliance with our lockfile versioning policy. In v0.4.28, we renamed it back to
/// `package.dev-dependencies`, with backwards compatibility for `package.dependency-groups`.
#[test]
fn lock_dev_dependencies_alias() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        dev-dependencies = ["iniconfig"]
        "#,
    )?;

    let lock = context.temp_dir.child("uv.lock");

    // Write a lockfile with `[package.dev-dependencies]`.
    lock.write_str(r#"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

        [package.dependency-groups]
        dev = [{ name = "iniconfig" }]

        [package.metadata]

        [package.metadata.dependency-groups]
        dev = [{ name = "iniconfig" }]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]
    "#)?;

    // Re-locking should be a no-op.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // If we add a package, re-locking should use `dependency-groups`.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [tool.uv]
        dev-dependencies = ["iniconfig"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Added typing-extensions v4.10.0
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "typing-extensions" },
        ]

        [package.dev-dependencies]
        dev = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "typing-extensions" }]

        [package.metadata.requires-dev]
        dev = [{ name = "iniconfig" }]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 },
        ]
        "###
        );
    });

    Ok(())
}

/// Lock a `pyproject.toml`, reorder the dependencies, and ensure that the lockfile is _not_ updated
/// on the next run.
#[test]
fn lock_reorder() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio", "iniconfig"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
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
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio" },
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "anyio" },
            { name = "iniconfig" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###);

    // Reorder the dependencies.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig", "anyio"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###);

    Ok(())
}

/// Ensure that `requires-python` bounds are correctly narrowed in the presence of upper-bound caps.
#[test]
fn lock_narrowed_python_version_upper() -> Result<()> {
    let context = TestContext::new("3.12");

    let dependency = context.temp_dir.child("dependency");
    dependency.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dependency"
        version = "0.1.0"
        requires-python = ">=3.10"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.7, <4"
        dependencies = ["dependency ; python_version >= '3.10'"]

        [tool.uv.sources]
        dependency = { path = "./dependency" }

        [tool.uv]
        environments = ["python_version >= '3.10'"]
        "#,
    )?;

    let lockfile = context.temp_dir.join("uv.lock");

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(&lockfile).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.7, <4"
        resolution-markers = [
            "python_full_version >= '3.10'",
        ]
        supported-markers = [
            "python_full_version >= '3.10'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "dependency"
        version = "0.1.0"
        source = { directory = "dependency" }
        dependencies = [
            { name = "iniconfig", marker = "python_full_version >= '3.10'" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig" }]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "dependency", marker = "python_full_version >= '3.10'" },
        ]

        [package.metadata]
        requires-dist = [{ name = "dependency", marker = "python_full_version >= '3.10'", directory = "dependency" }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    Ok(())
}

#[test]
fn lock_narrowed_python_version() -> Result<()> {
    let context = TestContext::new("3.12");

    let dependency = context.temp_dir.child("dependency");
    dependency.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dependency"
        version = "0.1.0"
        requires-python = ">=3.7"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.7"
        dependencies = ["dependency ; python_version < '3.9'", "dependency ; python_version > '3.10'"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sources]
        dependency = { path = "./dependency" }
        "#,
    )?;

    let lockfile = context.temp_dir.join("uv.lock");

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(&lockfile).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.7"
        resolution-markers = [
            "python_full_version >= '3.11'",
            "python_full_version >= '3.9' and python_full_version < '3.11'",
            "python_full_version < '3.9'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "dependency"
        version = "0.1.0"
        source = { directory = "dependency" }
        dependencies = [
            { name = "iniconfig", marker = "python_full_version < '3.9' or python_full_version >= '3.11'" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig" }]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "dependency", marker = "python_full_version < '3.9' or python_full_version >= '3.11'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "dependency", marker = "python_full_version < '3.9'", directory = "dependency" },
            { name = "dependency", marker = "python_full_version >= '3.11'", directory = "dependency" },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    Ok(())
}

/// When resolving, we should skip forks that have an upper-bound on Python version that's below
/// our `requires-python` constraint.
///
/// See: <https://github.com/astral-sh/uv/issues/6059>
#[test]
fn lock_exclude_unnecessary_python_forks() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "foo"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio ; sys_platform == 'darwin'",
            "anyio ; python_version > '3.10'"
        ]
        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform == 'darwin'",
            "sys_platform != 'darwin'",
        ]

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
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "foo"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "anyio", marker = "python_full_version >= '3.11'" },
            { name = "anyio", marker = "sys_platform == 'darwin'" },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    Ok(())
}

/// Lock with a user-provided constraint on the space of supported environments.
#[test]
fn lock_constrained_environment() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["black"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv]
        environments = "platform_system != 'Windows'"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    // Because we're _not_ locking for Windows, `colorama` should not be included.
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform != 'win32'",
        ]
        supported-markers = [
            "sys_platform != 'win32'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "black"
        version = "24.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "click", marker = "sys_platform != 'win32'" },
            { name = "mypy-extensions", marker = "sys_platform != 'win32'" },
            { name = "packaging", marker = "sys_platform != 'win32'" },
            { name = "pathspec", marker = "sys_platform != 'win32'" },
            { name = "platformdirs", marker = "sys_platform != 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/8f/5f/bac24a952668c7482cfdb4ebf91ba57a796c9da8829363a772040c1a3312/black-24.3.0.tar.gz", hash = "sha256:a0c9c4a0771afc6919578cec71ce82a3e31e054904e7197deacbc9382671c41f", size = 634292 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b6/c6/1d174efa9ff02b22d0124c73fc5f4d4fb006d0d9a081aadc354d05754a13/black-24.3.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:2818cf72dfd5d289e48f37ccfa08b460bf469e67fb7c4abb07edc2e9f16fb63f", size = 1600822 },
            { url = "https://files.pythonhosted.org/packages/d9/ed/704731afffe460b8ff0672623b40fce9fe569f2ee617c15857e4d4440a3a/black-24.3.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:4acf672def7eb1725f41f38bf6bf425c8237248bb0804faa3965c036f7672d11", size = 1429987 },
            { url = "https://files.pythonhosted.org/packages/a8/05/8dd038e30caadab7120176d4bc109b7ca2f4457f12eef746b0560a583458/black-24.3.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:c7ed6668cbbfcd231fa0dc1b137d3e40c04c7f786e626b405c62bcd5db5857e4", size = 1755319 },
            { url = "https://files.pythonhosted.org/packages/4d/ea/31770a7e49f3eedfd8cd7b35e78b3a3aaad860400f8673994bc988318135/black-24.3.0-py3-none-any.whl", hash = "sha256:41622020d7120e01d377f74249e677039d20e6344ff5851de8a10f11f513bf93", size = 201493 },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941 },
        ]

        [[package]]
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695 },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488 },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191 },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "black", marker = "sys_platform != 'win32'" },
        ]

        [package.metadata]
        requires-dist = [{ name = "black" }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    "###);

    // Re-run with `--offline`. We shouldn't need a network connection to validate an
    // already-correct lockfile with immutable metadata.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    "###);

    // Rewrite with a list, rather than a string.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["black"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv]
        environments = ["platform_system != 'Windows'"]
        "#,
    )?;

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    "###);

    // Re-lock without the environment constraint.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["black"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    // Re-run with `--locked`. This should fail.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided. To update the lockfile, run `uv lock`.
    "###);

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Added colorama v0.4.6
    "###);

    let lock = context.read("uv.lock");

    // Because we're locking for Windows, `colorama` should be included.
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "black"
        version = "24.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "click" },
            { name = "mypy-extensions" },
            { name = "packaging" },
            { name = "pathspec" },
            { name = "platformdirs" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/8f/5f/bac24a952668c7482cfdb4ebf91ba57a796c9da8829363a772040c1a3312/black-24.3.0.tar.gz", hash = "sha256:a0c9c4a0771afc6919578cec71ce82a3e31e054904e7197deacbc9382671c41f", size = 634292 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b6/c6/1d174efa9ff02b22d0124c73fc5f4d4fb006d0d9a081aadc354d05754a13/black-24.3.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:2818cf72dfd5d289e48f37ccfa08b460bf469e67fb7c4abb07edc2e9f16fb63f", size = 1600822 },
            { url = "https://files.pythonhosted.org/packages/d9/ed/704731afffe460b8ff0672623b40fce9fe569f2ee617c15857e4d4440a3a/black-24.3.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:4acf672def7eb1725f41f38bf6bf425c8237248bb0804faa3965c036f7672d11", size = 1429987 },
            { url = "https://files.pythonhosted.org/packages/a8/05/8dd038e30caadab7120176d4bc109b7ca2f4457f12eef746b0560a583458/black-24.3.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:c7ed6668cbbfcd231fa0dc1b137d3e40c04c7f786e626b405c62bcd5db5857e4", size = 1755319 },
            { url = "https://files.pythonhosted.org/packages/71/9d/e5fa1ff4ef1940be15a64883c0bb8d2fcf626efec996eab4ae5a8c691d2c/black-24.3.0-cp312-cp312-win_amd64.whl", hash = "sha256:56f52cfbd3dabe2798d76dbdd299faa046a901041faf2cf33288bc4e6dae57b5", size = 1385180 },
            { url = "https://files.pythonhosted.org/packages/4d/ea/31770a7e49f3eedfd8cd7b35e78b3a3aaad860400f8673994bc988318135/black-24.3.0-py3-none-any.whl", hash = "sha256:41622020d7120e01d377f74249e677039d20e6344ff5851de8a10f11f513bf93", size = 201493 },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "colorama", marker = "sys_platform == 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941 },
        ]

        [[package]]
        name = "colorama"
        version = "0.4.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d8/53/6f443c9a4a8358a93a6792e2acffb9d9d5cb0a5cfd8802644b7b1c9a02e4/colorama-0.4.6.tar.gz", hash = "sha256:08695f5cb7ed6e0531a20572697297273c47b8cae5a63ffc6d6ed5c201be6e44", size = 27697 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d1/d6/3965ed04c63042e047cb6a3e6ed1a63a35087b6a609aa3a15ed8ac56c221/colorama-0.4.6-py2.py3-none-any.whl", hash = "sha256:4f1d9991f5acc0ca119f9d443620b77f9d6b33703e51011c16baf57afb285fc6", size = 25335 },
        ]

        [[package]]
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695 },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488 },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191 },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "black" },
        ]

        [package.metadata]
        requires-dist = [{ name = "black" }]
        "###
        );
    });

    Ok(())
}

/// Lock with a user-provided constraint on the space of supported environments, using a legacy
/// virtual workspace root.
#[test]
fn lock_constrained_environment_legacy() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [tool.uv.workspace]
        members = ["child"]

        [tool.uv]
        environments = "platform_system != 'Windows'"
        "#,
    )?;

    let child = context.temp_dir.child("child");
    child.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["black"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    // Because we're _not_ locking for Windows, `colorama` should not be included.
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform != 'win32'",
        ]
        supported-markers = [
            "sys_platform != 'win32'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        members = [
            "child",
        ]

        [[package]]
        name = "black"
        version = "24.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "click", marker = "sys_platform != 'win32'" },
            { name = "mypy-extensions", marker = "sys_platform != 'win32'" },
            { name = "packaging", marker = "sys_platform != 'win32'" },
            { name = "pathspec", marker = "sys_platform != 'win32'" },
            { name = "platformdirs", marker = "sys_platform != 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/8f/5f/bac24a952668c7482cfdb4ebf91ba57a796c9da8829363a772040c1a3312/black-24.3.0.tar.gz", hash = "sha256:a0c9c4a0771afc6919578cec71ce82a3e31e054904e7197deacbc9382671c41f", size = 634292 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b6/c6/1d174efa9ff02b22d0124c73fc5f4d4fb006d0d9a081aadc354d05754a13/black-24.3.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:2818cf72dfd5d289e48f37ccfa08b460bf469e67fb7c4abb07edc2e9f16fb63f", size = 1600822 },
            { url = "https://files.pythonhosted.org/packages/d9/ed/704731afffe460b8ff0672623b40fce9fe569f2ee617c15857e4d4440a3a/black-24.3.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:4acf672def7eb1725f41f38bf6bf425c8237248bb0804faa3965c036f7672d11", size = 1429987 },
            { url = "https://files.pythonhosted.org/packages/a8/05/8dd038e30caadab7120176d4bc109b7ca2f4457f12eef746b0560a583458/black-24.3.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:c7ed6668cbbfcd231fa0dc1b137d3e40c04c7f786e626b405c62bcd5db5857e4", size = 1755319 },
            { url = "https://files.pythonhosted.org/packages/4d/ea/31770a7e49f3eedfd8cd7b35e78b3a3aaad860400f8673994bc988318135/black-24.3.0-py3-none-any.whl", hash = "sha256:41622020d7120e01d377f74249e677039d20e6344ff5851de8a10f11f513bf93", size = 201493 },
        ]

        [[package]]
        name = "child"
        version = "0.1.0"
        source = { virtual = "child" }
        dependencies = [
            { name = "black", marker = "sys_platform != 'win32'" },
        ]

        [package.metadata]
        requires-dist = [{ name = "black" }]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941 },
        ]

        [[package]]
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695 },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488 },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191 },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    "###);

    // Re-run with `--offline`. We shouldn't need a network connection to validate an
    // already-correct lockfile with immutable metadata.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    "###);

    Ok(())
}

/// User-provided constraints must be disjoint.
#[test]
fn lock_overlapping_environment() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.8"
        dependencies = ["black"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv]
        environments = ["platform_system != 'Windows'", "python_version > '3.10'"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Supported environments must be disjoint, but the following markers overlap: `sys_platform != 'win32'` and `python_full_version >= '3.11'`.

    hint: replace `python_full_version >= '3.11'` with `python_full_version >= '3.11' and sys_platform == 'win32'`.
    "###);

    Ok(())
}

/// Lock a (legacy) non-project workspace root with forked dev dependencies.
#[test]
fn lock_non_project_fork() -> Result<()> {
    let context = TestContext::new("3.10");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [tool.uv.workspace]
        members = []

        [tool.uv]
        dev-dependencies = [
            "anyio < 3 ; python_version >= '3.11'",
            "anyio > 3 ; python_version < '3.11'",
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: No `requires-python` value found in the workspace. Defaulting to `>=3.10`.
    Resolved 6 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.10"
        resolution-markers = [
            "python_full_version >= '3.11'",
            "python_full_version < '3.11'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [
            { name = "anyio", marker = "python_full_version < '3.11'", specifier = ">3" },
            { name = "anyio", marker = "python_full_version >= '3.11'", specifier = "<3" },
        ]

        [[package]]
        name = "anyio"
        version = "2.2.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.11'",
        ]
        dependencies = [
            { name = "idna", marker = "python_full_version >= '3.11'" },
            { name = "sniffio", marker = "python_full_version >= '3.11'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/d3/e6/901a94731af20e7109415525666cb3753a2bd1edd19616c2730448dffd0d/anyio-2.2.0.tar.gz", hash = "sha256:4a41c5b3a65ed92e469d51b6fba3779301850ea2e352afcf9e36c46f21ee14a9", size = 97217 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/c3/b83a3c02c7d6f66932e9a72621d7f207cbfd2bd72b4c8931567ee386fb55/anyio-2.2.0-py3-none-any.whl", hash = "sha256:aa3da546ed17f097ca876c78024dea380a3b7fa80759abfdda59f12176a3dac8", size = 65320 },
        ]

        [[package]]
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version < '3.11'",
        ]
        dependencies = [
            { name = "exceptiongroup", marker = "python_full_version < '3.11'" },
            { name = "idna", marker = "python_full_version < '3.11'" },
            { name = "sniffio", marker = "python_full_version < '3.11'" },
            { name = "typing-extensions", marker = "python_full_version < '3.11'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "exceptiongroup"
        version = "1.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/8e/1c/beef724eaf5b01bb44b6338c8c3494eff7cab376fab4904cfbbc3585dc79/exceptiongroup-1.2.0.tar.gz", hash = "sha256:91f5c769735f051a4290d52edd0858999b57e5876e9f85937691bd4c9fa3ed68", size = 26264 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b8/9a/5028fd52db10e600f1c4674441b968cf2ea4959085bfb5b99fb1250e5f68/exceptiongroup-1.2.0-py3-none-any.whl", hash = "sha256:4bfd3996ac73b41e9b9628b04e079f193850720ea5945fc96a08633c66912f14", size = 16210 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: No `requires-python` value found in the workspace. Defaulting to `>=3.10`.
    Resolved 6 packages in [TIME]
    "###);

    // Re-run with `--offline`. We shouldn't need a network connection to validate an
    // already-correct lockfile with immutable metadata.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: No `requires-python` value found in the workspace. Defaulting to `>=3.10`.
    Resolved 6 packages in [TIME]
    "###);

    // Add `iniconfig`.
    pyproject_toml.write_str(
        r#"
        [tool.uv.workspace]
        members = []

        [tool.uv]
        dev-dependencies = [
            "anyio < 3 ; python_version >= '3.11'",
            "anyio > 3 ; python_version < '3.11'",
            "iniconfig"
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: No `requires-python` value found in the workspace. Defaulting to `>=3.10`.
    Resolved 7 packages in [TIME]
    Added iniconfig v2.0.0
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + anyio==4.3.0
     + exceptiongroup==1.2.0
     + idna==3.6
     + iniconfig==2.0.0
     + sniffio==1.3.1
     + typing-extensions==4.10.0
    "###);

    Ok(())
}

/// Lock a (legacy) non-project workspace root with a conditional dependency.
#[test]
fn lock_non_project_conditional() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [tool.uv.workspace]
        members = []

        [tool.uv]
        dev-dependencies = ["anyio > 3 ; sys_platform == 'linux'"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: No `requires-python` value found in the workspace. Defaulting to `>=3.12`.
    Resolved 3 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "anyio", marker = "sys_platform == 'linux'", specifier = ">3" }]

        [[package]]
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: No `requires-python` value found in the workspace. Defaulting to `>=3.12`.
    Resolved 3 packages in [TIME]
    "###);

    // Re-run with `--offline`. We shouldn't need a network connection to validate an
    // already-correct lockfile with immutable metadata.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: No `requires-python` value found in the workspace. Defaulting to `>=3.12`.
    Resolved 3 packages in [TIME]
    "###);

    Ok(())
}

/// Lock a (legacy) non-project workspace root with `dependency-group`.
#[test]
fn lock_non_project_group() -> Result<()> {
    let context = TestContext::new("3.10");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [tool.uv.workspace]
        members = []

        [tool.uv]
        dev-dependencies = ["anyio"]

        [dependency-groups]
        lint = ["iniconfig"]
        dev = ["typing-extensions"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: No `requires-python` value found in the workspace. Defaulting to `>=3.10`.
    Resolved 6 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.10"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [
            { name = "anyio" },
            { name = "iniconfig" },
            { name = "typing-extensions" },
        ]

        [[package]]
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "exceptiongroup", marker = "python_full_version < '3.11'" },
            { name = "idna" },
            { name = "sniffio" },
            { name = "typing-extensions", marker = "python_full_version < '3.11'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "exceptiongroup"
        version = "1.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/8e/1c/beef724eaf5b01bb44b6338c8c3494eff7cab376fab4904cfbbc3585dc79/exceptiongroup-1.2.0.tar.gz", hash = "sha256:91f5c769735f051a4290d52edd0858999b57e5876e9f85937691bd4c9fa3ed68", size = 26264 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b8/9a/5028fd52db10e600f1c4674441b968cf2ea4959085bfb5b99fb1250e5f68/exceptiongroup-1.2.0-py3-none-any.whl", hash = "sha256:4bfd3996ac73b41e9b9628b04e079f193850720ea5945fc96a08633c66912f14", size = 16210 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: No `requires-python` value found in the workspace. Defaulting to `>=3.10`.
    Resolved 6 packages in [TIME]
    "###);

    // Re-run with `--offline`. We shouldn't need a network connection to validate an
    // already-correct lockfile with immutable metadata.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: No `requires-python` value found in the workspace. Defaulting to `>=3.10`.
    Resolved 6 packages in [TIME]
    "###);

    Ok(())
}

/// Lock a (legacy) non-project workspace root with `tool.uv.sources`.
#[test]
fn lock_non_project_sources() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [tool.uv.workspace]
        members = []

        [tool.uv]
        dev-dependencies = ["idna"]

        [tool.uv.sources]
        idna = { url = "https://files.pythonhosted.org/packages/d7/77/ff688d1504cdc4db2a938e2b7b9adee5dd52e34efbd2431051efc9984de9/idna-3.2-py3-none-any.whl" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: No `requires-python` value found in the workspace. Defaulting to `>=3.12`.
    Resolved 1 package in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "idna", url = "https://files.pythonhosted.org/packages/d7/77/ff688d1504cdc4db2a938e2b7b9adee5dd52e34efbd2431051efc9984de9/idna-3.2-py3-none-any.whl" }]

        [[package]]
        name = "idna"
        version = "3.2"
        source = { url = "https://files.pythonhosted.org/packages/d7/77/ff688d1504cdc4db2a938e2b7b9adee5dd52e34efbd2431051efc9984de9/idna-3.2-py3-none-any.whl" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d7/77/ff688d1504cdc4db2a938e2b7b9adee5dd52e34efbd2431051efc9984de9/idna-3.2-py3-none-any.whl", hash = "sha256:14475042e284991034cb48e06f6851428fb14c4dc953acd9be9a5e95c7b6dd7a" },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: No `requires-python` value found in the workspace. Defaulting to `>=3.12`.
    Resolved 1 package in [TIME]
    "###);

    // Re-run with `--offline`. We shouldn't need a network connection to validate an
    // already-correct lockfile with immutable metadata.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: No `requires-python` value found in the workspace. Defaulting to `>=3.12`.
    Resolved 1 package in [TIME]
    "###);

    Ok(())
}

/// `coverage` defines a `toml` extra, but it doesn't enable any dependencies after Python 3.11.
#[test]
fn lock_dropped_dev_extra() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv]
        dev-dependencies = ["coverage[toml]"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "coverage"
        version = "7.4.4"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/d5/f809d8b630cf4c11fe490e20037a343d12a74ec2783c6cdb5aee725e7137/coverage-7.4.4.tar.gz", hash = "sha256:c901df83d097649e257e803be22592aedfd5182f07b3cc87d640bbb9afd50f49", size = 783727 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a0/de/a54b245e781bfd6f3fd7ce5566a695686b5c25ee7c743f514e7634428972/coverage-7.4.4-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:201bef2eea65e0e9c56343115ba3814e896afe6d36ffd37bab783261db430f76", size = 206409 },
            { url = "https://files.pythonhosted.org/packages/88/92/07f9c593cd27e3c595b8cb83b95adad8c9ba3d611debceed097a5fd6be4b/coverage-7.4.4-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:41c9c5f3de16b903b610d09650e5e27adbfa7f500302718c9ffd1c12cf9d6818", size = 206568 },
            { url = "https://files.pythonhosted.org/packages/41/6d/e142c823e5d4b24481f990da4cf9d2d577a6f4e1fb6faf39d9a4e42b1d43/coverage-7.4.4-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:d898fe162d26929b5960e4e138651f7427048e72c853607f2b200909794ed978", size = 238920 },
            { url = "https://files.pythonhosted.org/packages/30/1a/105f0139df6a2adbcaa0c110711a46dbd9f59e93a09ca15a97d59c2564f2/coverage-7.4.4-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:3ea79bb50e805cd6ac058dfa3b5c8f6c040cb87fe83de10845857f5535d1db70", size = 236288 },
            { url = "https://files.pythonhosted.org/packages/98/79/185cb42910b6a2b2851980407c8445ac0da0750dff65e420e86f973c8396/coverage-7.4.4-cp312-cp312-manylinux_2_5_x86_64.manylinux1_x86_64.manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:ce4b94265ca988c3f8e479e741693d143026632672e3ff924f25fab50518dd51", size = 238223 },
            { url = "https://files.pythonhosted.org/packages/92/12/2303d1c543a11ea060dbc7144ed3174fc09107b5dd333649415c95ede58b/coverage-7.4.4-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:00838a35b882694afda09f85e469c96367daa3f3f2b097d846a7216993d37f4c", size = 245161 },
            { url = "https://files.pythonhosted.org/packages/96/5a/7d0e945c4759fe9d19aad1679dd3096aeb4cb9fcf0062fe24554dc4787b8/coverage-7.4.4-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:fdfafb32984684eb03c2d83e1e51f64f0906b11e64482df3c5db936ce3839d48", size = 243066 },
            { url = "https://files.pythonhosted.org/packages/f4/1b/79cdb7b11bbbd6540a536ac79412904b5c1f8903d5c1330084212afa8ceb/coverage-7.4.4-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:69eb372f7e2ece89f14751fbcbe470295d73ed41ecd37ca36ed2eb47512a6ab9", size = 244805 },
            { url = "https://files.pythonhosted.org/packages/af/7f/54dc676e7e63549838a3a7b95a8e11df80441bf7d64c6ce8f1cdbc0d1ff0/coverage-7.4.4-cp312-cp312-win32.whl", hash = "sha256:137eb07173141545e07403cca94ab625cc1cc6bc4c1e97b6e3846270e7e1fea0", size = 208590 },
            { url = "https://files.pythonhosted.org/packages/46/c4/1dfe76d96034a347d717a2392b004d42d45934cb94efa362ad41ca871f6e/coverage-7.4.4-cp312-cp312-win_amd64.whl", hash = "sha256:d71eec7d83298f1af3326ce0ff1d0ea83c7cb98f72b577097f9083b20bdaf05e", size = 209415 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }

        [package.dev-dependencies]
        dev = [
            { name = "coverage" },
        ]

        [package.metadata]

        [package.metadata.requires-dev]
        dev = [{ name = "coverage", extras = ["toml"] }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Re-run with `--offline`. We shouldn't need a network connection to validate an
    // already-correct lockfile with immutable metadata.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + coverage==7.4.4
     + project==0.1.0 (from file://[TEMP_DIR]/)
    "###);

    Ok(())
}

/// Lock with an empty (but existent) `tool.uv.dev-dependencies` group.
#[test]
fn lock_empty_dev_dependencies() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv]
        dev-dependencies = []
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig" }]

        [package.metadata.requires-dev]
        dev = []
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Re-run with `--offline`. We shouldn't need a network connection to validate an
    // already-correct lockfile with immutable metadata.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     + project==0.1.0 (from file://[TEMP_DIR]/)
    "###);

    Ok(())
}

/// Lock with an empty (but existent) `dependency-groups` group.
#[test]
fn lock_empty_dependency_group() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [dependency-groups]
        empty = []
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig" }]

        [package.metadata.requires-dev]
        empty = []
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Re-run with `--offline`. We shouldn't need a network connection to validate an
    // already-correct lockfile with immutable metadata.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     + project==0.1.0 (from file://[TEMP_DIR]/)
    "###);

    Ok(())
}

/// Use a trailing slash on the declared index.
#[test]
fn lock_trailing_slash() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv]
        index-url = "https://pypi.org/simple/"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "3.7.0"
        source = { registry = "https://pypi.org/simple/" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/c6/b3/fefbf7e78ab3b805dec67d698dc18dd505af7a18a8dd08868c9b4fa736b5/anyio-3.7.0.tar.gz", hash = "sha256:275d9973793619a5374e1c89a4f4ad3f4b0a5510a2b5b939444bee8f4c4d37ce", size = 142737 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/fe/7ce1926952c8a403b35029e194555558514b365ad77d75125f521a2bec62/anyio-3.7.0-py3-none-any.whl", hash = "sha256:eddca883c4175f14df8aedce21054bfca3adb70ffe76a9f607aef9d7fa2ea7f0", size = 80873 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple/" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio", specifier = "==3.7.0" }]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple/" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    // Re-run with `--offline`. We shouldn't need a network connection to validate an
    // already-correct lockfile with immutable metadata.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.1
    "###);

    Ok(())
}

#[test]
fn lock_invalid_index() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0", "iniconfig==2.0.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sources]
        iniconfig = { index = "internal proxy" }

        [[tool.uv.index]]
        name = "internal proxy"
        url = "https://test.pypi.org/simple"
        explicit = true
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: Failed to parse `pyproject.toml` during settings discovery:
      TOML parse error at line 16, column 16
         |
      16 |         name = "internal proxy"
         |                ^^^^^^^^^^^^^^^^
      Index names may only contain letters, digits, hyphens, underscores, and periods, but found unsupported character (` `) in: `internal proxy`

    error: Failed to parse: `pyproject.toml`
      Caused by: TOML parse error at line 13, column 31
       |
    13 |         iniconfig = { index = "internal proxy" }
       |                               ^^^^^^^^^^^^^^^^
    Index names may only contain letters, digits, hyphens, underscores, and periods, but found unsupported character (` `) in: `internal proxy`
    "###);

    Ok(())
}

#[test]
fn lock_explicit_index() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0", "iniconfig==2.0.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sources]
        iniconfig = { index = "test" }

        [[tool.uv.index]]
        name = "test"
        url = "https://test.pypi.org/simple"
        explicit = true
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
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
        sdist = { url = "https://files.pythonhosted.org/packages/c6/b3/fefbf7e78ab3b805dec67d698dc18dd505af7a18a8dd08868c9b4fa736b5/anyio-3.7.0.tar.gz", hash = "sha256:275d9973793619a5374e1c89a4f4ad3f4b0a5510a2b5b939444bee8f4c4d37ce", size = 142737 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/fe/7ce1926952c8a403b35029e194555558514b365ad77d75125f521a2bec62/anyio-3.7.0-py3-none-any.whl", hash = "sha256:eddca883c4175f14df8aedce21054bfca3adb70ffe76a9f607aef9d7fa2ea7f0", size = 80873 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://test.pypi.org/simple" }
        sdist = { url = "https://test-files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://test-files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio" },
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "anyio", specifier = "==3.7.0" },
            { name = "iniconfig", specifier = "==2.0.0", index = "https://test.pypi.org/simple" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    Ok(())
}

#[test]
fn lock_explicit_default_index() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.sources]
        iniconfig = { index = "test" }

        [[tool.uv.index]]
        name = "test"
        url = "https://test.pypi.org/simple"
        explicit = true
        default = true
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://test.pypi.org/simple" }
        sdist = { url = "https://test-files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://test-files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig", specifier = "==2.0.0", index = "https://test.pypi.org/simple" }]
        "###
        );
    });

    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [[tool.uv.index]]
        name = "test"
        url = "https://test.pypi.org/simple"
        explicit = true
        default = true
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().arg("--verbose"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    DEBUG uv [VERSION] ([COMMIT] DATE)
    DEBUG Found workspace root: `[TEMP_DIR]/`
    DEBUG Adding current workspace member: `[TEMP_DIR]/`
    DEBUG Using Python request `>=3.12` from `requires-python` metadata
    DEBUG The virtual environment's Python version satisfies `>=3.12`
    DEBUG Using request timeout of [TIME]
    DEBUG Found static `pyproject.toml` for: project @ file://[TEMP_DIR]/
    DEBUG No workspace root found, using project root
    DEBUG Ignoring existing lockfile due to mismatched `requires-dist` for: `project==0.1.0`
      Expected: {Requirement { name: PackageName("anyio"), extras: [], groups: [], marker: true, source: Registry { specifier: VersionSpecifiers([]), index: None, conflict: None }, origin: None }}
      Actual: {Requirement { name: PackageName("iniconfig"), extras: [], groups: [], marker: true, source: Registry { specifier: VersionSpecifiers([VersionSpecifier { operator: Equal, version: "2.0.0" }]), index: Some(Url { scheme: "https", cannot_be_a_base: false, username: "", password: None, host: Some(Domain("test.pypi.org")), port: None, path: "/simple", query: None, fragment: None }), conflict: None }, origin: None }}
    DEBUG Solving with installed Python version: 3.12.[X]
    DEBUG Solving with target Python version: >=3.12
    DEBUG Adding direct dependency: project*
    DEBUG Searching for a compatible version of project @ file://[TEMP_DIR]/ (*)
    DEBUG Adding transitive dependency for project==0.1.0: anyio*
    DEBUG Searching for a compatible version of anyio (*)
    DEBUG No compatible version found for: anyio
    DEBUG Recording unit propagation conflict of anyio from incompatibility of (project)
    DEBUG Searching for a compatible version of project @ file://[TEMP_DIR]/ (<0.1.0 | >0.1.0)
    DEBUG No compatible version found for: project
      × No solution found when resolving dependencies:
      ╰─▶ Because anyio was not found in the provided package locations and your project depends on anyio, we can conclude that your project's requirements are unsatisfiable.

          hint: Packages were unavailable because index lookups were disabled and no additional package locations were provided (try: `--find-links <uri>`)
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://test.pypi.org/simple" }
        sdist = { url = "https://test-files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://test-files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig", specifier = "==2.0.0", index = "https://test.pypi.org/simple" }]
        "###
        );
    });

    Ok(())
}

#[test]
fn lock_named_index() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [[tool.uv.index]]
        name = "pytorch"
        url = "https://download.pytorch.org/whl/cu121"
        explicit = true

        [[tool.uv.index]]
        name = "heron"
        url = "https://pypi-proxy.fly.dev/simple"

        [[tool.uv.index]]
        name = "test"
        url = "https://test.pypi.org/simple"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "typing-extensions" },
        ]

        [package.metadata]
        requires-dist = [{ name = "typing-extensions" }]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi-proxy.fly.dev/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 },
        ]
        "###
        );
    });

    Ok(())
}

#[test]
fn lock_default_index() -> Result<()> {
    let context = TestContext::new("3.12");

    // If an index is included, PyPI will still be used as the default index.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [[tool.uv.index]]
        name = "pytorch"
        url = "https://download.pytorch.org/whl/cu121"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig" }]
        "###
        );
    });

    // Unless that index is marked as the default.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [[tool.uv.index]]
        name = "pytorch"
        url = "https://download.pytorch.org/whl/cu121"
        default = true
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because iniconfig was not found in the package registry and your project depends on iniconfig, we can conclude that your project's requirements are unsatisfiable.
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig" }]
        "###
        );
    });

    Ok(())
}

#[test]
fn lock_named_index_cli() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["jinja2==3.1.2"]

        [tool.uv.sources]
        jinja2 = { index = "pytorch" }
        "#,
    )?;

    // The package references a non-existent index.
    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `project @ file://[TEMP_DIR]/`
      ├─▶ Failed to parse entry: `jinja2`
      ╰─▶ Package `jinja2` references an undeclared index: `pytorch`
    "###);

    // But it's fine if it comes from the CLI.
    uv_snapshot!(context.filters(), context.lock().arg("--index").arg("pytorch=https://download.pytorch.org/whl/cu121").env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [[package]]
        name = "jinja2"
        version = "3.1.2"
        source = { registry = "https://download.pytorch.org/whl/cu121" }
        dependencies = [
            { name = "markupsafe" },
        ]
        wheels = [
            { url = "https://download.pytorch.org/whl/Jinja2-3.1.2-py3-none-any.whl", hash = "sha256:6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61" },
        ]

        [[package]]
        name = "markupsafe"
        version = "3.0.2"
        source = { registry = "https://download.pytorch.org/whl/cu121" }
        wheels = [
            { url = "https://download.pytorch.org/whl/MarkupSafe-3.0.2-cp313-cp313-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:15ab75ef81add55874e7ab7055e9c397312385bd9ced94920f2802310c930396" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "jinja2" },
        ]

        [package.metadata]
        requires-dist = [{ name = "jinja2", specifier = "==3.1.2", index = "https://download.pytorch.org/whl/cu121" }]
        "###
        );
    });

    Ok(())
}

/// If a name is reused, the higher-priority index should "overwrite" the lower-priority index.
/// In other words, the lower-priority index should be ignored entirely during implicit resolution.
///
/// In this test, we should use PyPI (the default index) and ignore `https://example.com` entirely.
/// (Querying `https://example.com` would fail with a 500.)
#[test]
fn lock_repeat_named_index() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [[tool.uv.index]]
        name = "pytorch"
        url = "https://download.pytorch.org/whl/cu121"

        [[tool.uv.index]]
        name = "pytorch"
        url = "https://example.com"
        "#,
    )?;

    // Fall back to PyPI, since `iniconfig` doesn't exist on the PyTorch index.
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig" }]
        "###
        );
    });

    Ok(())
}

/// If a name is reused, the higher-priority index should "overwrite" the lower-priority index.
/// This includes names passed in via the CLI.
#[test]
fn lock_repeat_named_index_cli() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["jinja2==3.1.2"]

        [tool.uv]
        constraint-dependencies = ["markupsafe<3"]

        [[tool.uv.index]]
        name = "pytorch"
        url = "https://download.pytorch.org/whl/cu121"
        "#,
    )?;

    // Resolve to the PyTorch index.
    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [manifest]
        constraints = [{ name = "markupsafe", specifier = "<3" }]

        [[package]]
        name = "jinja2"
        version = "3.1.2"
        source = { registry = "https://download.pytorch.org/whl/cu121" }
        dependencies = [
            { name = "markupsafe" },
        ]
        wheels = [
            { url = "https://download.pytorch.org/whl/Jinja2-3.1.2-py3-none-any.whl", hash = "sha256:6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61" },
        ]

        [[package]]
        name = "markupsafe"
        version = "2.1.5"
        source = { registry = "https://download.pytorch.org/whl/cu121" }
        wheels = [
            { url = "https://download.pytorch.org/whl/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:8dec4936e9c3100156f8a2dc89c4b88d5c435175ff03413b443469c7c8c5f4d1" },
            { url = "https://download.pytorch.org/whl/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:3c6b973f22eb18a789b1460b4b91bf04ae3f0c4234a0a6aa6b0a92f6f7b951d4" },
            { url = "https://download.pytorch.org/whl/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ac07bad82163452a6884fe8fa0963fb98c2346ba78d779ec06bd7a6262132aee" },
            { url = "https://download.pytorch.org/whl/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f5dfb42c4604dddc8e4305050aa6deb084540643ed5804d7455b5df8fe16f5e5" },
            { url = "https://download.pytorch.org/whl/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ea3d8a3d18833cf4304cd2fc9cbb1efe188ca9b5efef2bdac7adc20594a0e46b" },
            { url = "https://download.pytorch.org/whl/MarkupSafe-2.1.5-cp312-cp312-win_amd64.whl", hash = "sha256:823b65d8706e32ad2df51ed89496147a42a2a6e01c13cfb6ffb8b1e92bc910bb" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "jinja2" },
        ]

        [package.metadata]
        requires-dist = [{ name = "jinja2", specifier = "==3.1.2" }]
        "###
        );
    });

    // Resolve to PyPI, since the PyTorch index is replaced by the Packse index, which doesn't
    // include `jinja2`.
    uv_snapshot!(context.filters(), context.lock().arg("--index").arg(format!("pytorch={}", packse_index_url())).env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [manifest]
        constraints = [{ name = "markupsafe", specifier = "<3" }]

        [[package]]
        name = "jinja2"
        version = "3.1.2"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "markupsafe" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/7a/ff/75c28576a1d900e87eb6335b063fab47a8ef3c8b4d88524c4bf78f670cce/Jinja2-3.1.2.tar.gz", hash = "sha256:31351a702a408a9e7595a8fc6150fc3f43bb6bf7e319770cbc0db9df9437e852", size = 268239 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/bc/c3/f068337a370801f372f2f8f6bad74a5c140f6fda3d9de154052708dd3c65/Jinja2-3.1.2-py3-none-any.whl", hash = "sha256:6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61", size = 133101 },
        ]

        [[package]]
        name = "markupsafe"
        version = "2.1.5"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/87/5b/aae44c6655f3801e81aa3eef09dbbf012431987ba564d7231722f68df02d/MarkupSafe-2.1.5.tar.gz", hash = "sha256:d283d37a890ba4c1ae73ffadf8046435c76e7bc2247bbb63c00bd1a709c6544b", size = 19384 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/53/bd/583bf3e4c8d6a321938c13f49d44024dbe5ed63e0a7ba127e454a66da974/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:8dec4936e9c3100156f8a2dc89c4b88d5c435175ff03413b443469c7c8c5f4d1", size = 18215 },
            { url = "https://files.pythonhosted.org/packages/48/d6/e7cd795fc710292c3af3a06d80868ce4b02bfbbf370b7cee11d282815a2a/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:3c6b973f22eb18a789b1460b4b91bf04ae3f0c4234a0a6aa6b0a92f6f7b951d4", size = 14069 },
            { url = "https://files.pythonhosted.org/packages/51/b5/5d8ec796e2a08fc814a2c7d2584b55f889a55cf17dd1a90f2beb70744e5c/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ac07bad82163452a6884fe8fa0963fb98c2346ba78d779ec06bd7a6262132aee", size = 29452 },
            { url = "https://files.pythonhosted.org/packages/0a/0d/2454f072fae3b5a137c119abf15465d1771319dfe9e4acbb31722a0fff91/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f5dfb42c4604dddc8e4305050aa6deb084540643ed5804d7455b5df8fe16f5e5", size = 28462 },
            { url = "https://files.pythonhosted.org/packages/2d/75/fd6cb2e68780f72d47e6671840ca517bda5ef663d30ada7616b0462ad1e3/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ea3d8a3d18833cf4304cd2fc9cbb1efe188ca9b5efef2bdac7adc20594a0e46b", size = 27869 },
            { url = "https://files.pythonhosted.org/packages/b0/81/147c477391c2750e8fc7705829f7351cf1cd3be64406edcf900dc633feb2/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:d050b3361367a06d752db6ead6e7edeb0009be66bc3bae0ee9d97fb326badc2a", size = 33906 },
            { url = "https://files.pythonhosted.org/packages/8b/ff/9a52b71839d7a256b563e85d11050e307121000dcebc97df120176b3ad93/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:bec0a414d016ac1a18862a519e54b2fd0fc8bbfd6890376898a6c0891dd82e9f", size = 32296 },
            { url = "https://files.pythonhosted.org/packages/88/07/2dc76aa51b481eb96a4c3198894f38b480490e834479611a4053fbf08623/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:58c98fee265677f63a4385256a6d7683ab1832f3ddd1e66fe948d5880c21a169", size = 33038 },
            { url = "https://files.pythonhosted.org/packages/96/0c/620c1fb3661858c0e37eb3cbffd8c6f732a67cd97296f725789679801b31/MarkupSafe-2.1.5-cp312-cp312-win32.whl", hash = "sha256:8590b4ae07a35970728874632fed7bd57b26b0102df2d2b233b6d9d82f6c62ad", size = 16572 },
            { url = "https://files.pythonhosted.org/packages/3f/14/c3554d512d5f9100a95e737502f4a2323a1959f6d0d01e0d0997b35f7b10/MarkupSafe-2.1.5-cp312-cp312-win_amd64.whl", hash = "sha256:823b65d8706e32ad2df51ed89496147a42a2a6e01c13cfb6ffb8b1e92bc910bb", size = 17127 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "jinja2" },
        ]

        [package.metadata]
        requires-dist = [{ name = "jinja2", specifier = "==3.1.2" }]
        "###
        );
    });

    Ok(())
}

/// Lock a project with `package = false`, making it a virtual project.
#[test]
fn lock_explicit_virtual_project() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["black"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv]
        package = false
        dev-dependencies = [
            "anyio"
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 11 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
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
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "black"
        version = "24.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "click" },
            { name = "mypy-extensions" },
            { name = "packaging" },
            { name = "pathspec" },
            { name = "platformdirs" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/8f/5f/bac24a952668c7482cfdb4ebf91ba57a796c9da8829363a772040c1a3312/black-24.3.0.tar.gz", hash = "sha256:a0c9c4a0771afc6919578cec71ce82a3e31e054904e7197deacbc9382671c41f", size = 634292 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b6/c6/1d174efa9ff02b22d0124c73fc5f4d4fb006d0d9a081aadc354d05754a13/black-24.3.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:2818cf72dfd5d289e48f37ccfa08b460bf469e67fb7c4abb07edc2e9f16fb63f", size = 1600822 },
            { url = "https://files.pythonhosted.org/packages/d9/ed/704731afffe460b8ff0672623b40fce9fe569f2ee617c15857e4d4440a3a/black-24.3.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:4acf672def7eb1725f41f38bf6bf425c8237248bb0804faa3965c036f7672d11", size = 1429987 },
            { url = "https://files.pythonhosted.org/packages/a8/05/8dd038e30caadab7120176d4bc109b7ca2f4457f12eef746b0560a583458/black-24.3.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:c7ed6668cbbfcd231fa0dc1b137d3e40c04c7f786e626b405c62bcd5db5857e4", size = 1755319 },
            { url = "https://files.pythonhosted.org/packages/71/9d/e5fa1ff4ef1940be15a64883c0bb8d2fcf626efec996eab4ae5a8c691d2c/black-24.3.0-cp312-cp312-win_amd64.whl", hash = "sha256:56f52cfbd3dabe2798d76dbdd299faa046a901041faf2cf33288bc4e6dae57b5", size = 1385180 },
            { url = "https://files.pythonhosted.org/packages/4d/ea/31770a7e49f3eedfd8cd7b35e78b3a3aaad860400f8673994bc988318135/black-24.3.0-py3-none-any.whl", hash = "sha256:41622020d7120e01d377f74249e677039d20e6344ff5851de8a10f11f513bf93", size = 201493 },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "colorama", marker = "sys_platform == 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941 },
        ]

        [[package]]
        name = "colorama"
        version = "0.4.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d8/53/6f443c9a4a8358a93a6792e2acffb9d9d5cb0a5cfd8802644b7b1c9a02e4/colorama-0.4.6.tar.gz", hash = "sha256:08695f5cb7ed6e0531a20572697297273c47b8cae5a63ffc6d6ed5c201be6e44", size = 27697 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d1/d6/3965ed04c63042e047cb6a3e6ed1a63a35087b6a609aa3a15ed8ac56c221/colorama-0.4.6-py2.py3-none-any.whl", hash = "sha256:4f1d9991f5acc0ca119f9d443620b77f9d6b33703e51011c16baf57afb285fc6", size = 25335 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695 },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488 },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191 },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "black" },
        ]

        [package.dev-dependencies]
        dev = [
            { name = "anyio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "black" }]

        [package.metadata.requires-dev]
        dev = [{ name = "anyio" }]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 11 packages in [TIME]
    "###);

    // Re-run with `--offline`. We shouldn't need a network connection to validate an
    // already-correct lockfile with immutable metadata.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 11 packages in [TIME]
    "###);

    // Install from the lockfile. The virtual project should _not_ be installed.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 9 packages in [TIME]
    Installed 9 packages in [TIME]
     + anyio==4.3.0
     + black==24.3.0
     + click==8.1.7
     + idna==3.6
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
     + sniffio==1.3.1
    "###);

    Ok(())
}

/// Lock a project that is implicitly virtual (by way of omitting `build-system`).
#[test]
fn lock_implicit_virtual_project() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["black"]

        [tool.uv]
        dev-dependencies = [
            "anyio"
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 11 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
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
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "black"
        version = "24.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "click" },
            { name = "mypy-extensions" },
            { name = "packaging" },
            { name = "pathspec" },
            { name = "platformdirs" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/8f/5f/bac24a952668c7482cfdb4ebf91ba57a796c9da8829363a772040c1a3312/black-24.3.0.tar.gz", hash = "sha256:a0c9c4a0771afc6919578cec71ce82a3e31e054904e7197deacbc9382671c41f", size = 634292 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b6/c6/1d174efa9ff02b22d0124c73fc5f4d4fb006d0d9a081aadc354d05754a13/black-24.3.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:2818cf72dfd5d289e48f37ccfa08b460bf469e67fb7c4abb07edc2e9f16fb63f", size = 1600822 },
            { url = "https://files.pythonhosted.org/packages/d9/ed/704731afffe460b8ff0672623b40fce9fe569f2ee617c15857e4d4440a3a/black-24.3.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:4acf672def7eb1725f41f38bf6bf425c8237248bb0804faa3965c036f7672d11", size = 1429987 },
            { url = "https://files.pythonhosted.org/packages/a8/05/8dd038e30caadab7120176d4bc109b7ca2f4457f12eef746b0560a583458/black-24.3.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:c7ed6668cbbfcd231fa0dc1b137d3e40c04c7f786e626b405c62bcd5db5857e4", size = 1755319 },
            { url = "https://files.pythonhosted.org/packages/71/9d/e5fa1ff4ef1940be15a64883c0bb8d2fcf626efec996eab4ae5a8c691d2c/black-24.3.0-cp312-cp312-win_amd64.whl", hash = "sha256:56f52cfbd3dabe2798d76dbdd299faa046a901041faf2cf33288bc4e6dae57b5", size = 1385180 },
            { url = "https://files.pythonhosted.org/packages/4d/ea/31770a7e49f3eedfd8cd7b35e78b3a3aaad860400f8673994bc988318135/black-24.3.0-py3-none-any.whl", hash = "sha256:41622020d7120e01d377f74249e677039d20e6344ff5851de8a10f11f513bf93", size = 201493 },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "colorama", marker = "sys_platform == 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941 },
        ]

        [[package]]
        name = "colorama"
        version = "0.4.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d8/53/6f443c9a4a8358a93a6792e2acffb9d9d5cb0a5cfd8802644b7b1c9a02e4/colorama-0.4.6.tar.gz", hash = "sha256:08695f5cb7ed6e0531a20572697297273c47b8cae5a63ffc6d6ed5c201be6e44", size = 27697 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d1/d6/3965ed04c63042e047cb6a3e6ed1a63a35087b6a609aa3a15ed8ac56c221/colorama-0.4.6-py2.py3-none-any.whl", hash = "sha256:4f1d9991f5acc0ca119f9d443620b77f9d6b33703e51011c16baf57afb285fc6", size = 25335 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695 },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488 },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191 },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "black" },
        ]

        [package.dev-dependencies]
        dev = [
            { name = "anyio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "black" }]

        [package.metadata.requires-dev]
        dev = [{ name = "anyio" }]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 11 packages in [TIME]
    "###);

    // Re-run with `--offline`. We shouldn't need a network connection to validate an
    // already-correct lockfile with immutable metadata.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 11 packages in [TIME]
    "###);

    // Install from the lockfile. The virtual project should _not_ be installed.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 9 packages in [TIME]
    Installed 9 packages in [TIME]
     + anyio==4.3.0
     + black==24.3.0
     + click==8.1.7
     + idna==3.6
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
     + sniffio==1.3.1
    "###);

    Ok(())
}

/// Lock a project that has a path dependency that is implicitly virtual (by way of omitting
/// `build-system`).
#[test]
fn lock_implicit_virtual_path() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio >3", "child"]

        [tool.uv.sources]
        child = { path = "./child" }
        "#,
    )?;

    let child = context.temp_dir.child("child");
    child.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig >1"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
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
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "child"
        version = "0.1.0"
        source = { virtual = "child" }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig", specifier = ">1" }]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "anyio" },
            { name = "child" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "anyio", specifier = ">3" },
            { name = "child", virtual = "child" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "###);

    // Re-run with `--offline`. We shouldn't need a network connection to validate an
    // already-correct lockfile with immutable metadata.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "###);

    // Install from the lockfile. The virtual project should _not_ be installed.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==4.3.0
     + idna==3.6
     + iniconfig==2.0.0
     + sniffio==1.3.1
    "###);

    Ok(())
}

#[test]
fn lock_conflicting_environment() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv]
        environments = ["python_version < '3.11'"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Environment markers `python_full_version < '3.11'` don't overlap with Python requirement `>=3.12`
    "###);

    Ok(())
}

#[test]
fn lock_split_python_environment() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "foo"
        version = "0.1.0"
        requires-python = ">=3.7"
        dependencies = ["uv ; python_version >= '3.8'"]

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"

        [tool.uv]
        environments = ["python_version < '3.8'", "python_version >= '3.8'"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.7"
        resolution-markers = [
            "python_full_version < '3.8'",
            "python_full_version >= '3.8'",
        ]
        supported-markers = [
            "python_full_version < '3.8'",
            "python_full_version >= '3.8'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "foo"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "uv", marker = "python_full_version >= '3.8'" },
        ]

        [package.metadata]
        requires-dist = [{ name = "uv", marker = "python_full_version >= '3.8'" }]

        [[package]]
        name = "uv"
        version = "0.1.24"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/da/dc/73cc9792f5e5362612bb9fadd1b158f941b7bc9d47016416f36d077b995b/uv-0.1.24.tar.gz", hash = "sha256:1f8abf3330570acbf6188da635c4fe9cc936f9f36b49ce4992a2df56b2155421", size = 598670 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c8/0c/f3b9a32eddeb828c525c7d8c4bdfe1e456721e7295d2601d0ded841ec7be/uv-0.1.24-py3-none-linux_armv6l.whl", hash = "sha256:d87a9c4b35a4a1347586ec8f194045d96e314b822a66c48eebb5787d9c49461a", size = 9743060 },
            { url = "https://files.pythonhosted.org/packages/1e/ef/90fc17103183c23e8e5142c455b1ca8c1c38142c89c8aaa27f7ab37c34f5/uv-0.1.24-py3-none-macosx_10_12_x86_64.macosx_11_0_arm64.macosx_10_12_universal2.whl", hash = "sha256:33d74c4c67df34de18c4318a6c568efef9dfab6d05332b1d1eddc8e516fc8806", size = 20937935 },
            { url = "https://files.pythonhosted.org/packages/86/9e/3be68f7e00f8b43ce9b0b26fbdf85df5901601bbc7399b5d968e7d85b6b2/uv-0.1.24-py3-none-macosx_10_12_x86_64.whl", hash = "sha256:76a8abb05b957beddf7fb35b4da5e8b1d43326a423615eefa043f61f9720f381", size = 10484601 },
            { url = "https://files.pythonhosted.org/packages/2e/a6/3bf8bfaf67b7547b4cbb604c582c5b1d7becd280f85a310b1c4aa7af1d9d/uv-0.1.24-py3-none-manylinux_2_17_armv7l.manylinux2014_armv7l.whl", hash = "sha256:735c11c977dfe044ee5395c523503b9ffd9d055e924a86dba75130f60411a5d2", size = 9653488 },
            { url = "https://files.pythonhosted.org/packages/46/57/34601c7274bcad1a60ac3481f46e457efad632899f8608ff290cfc19031b/uv-0.1.24-py3-none-manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:2e4d09bed7759253a81c1141e01d58242fb964e6eea85ed2f3d15f6597c504fa", size = 11536947 },
            { url = "https://files.pythonhosted.org/packages/a4/2f/157fae38fbc5dc7570df63d484ec84cd7ac6287b6614145929221d6f6d35/uv-0.1.24-py3-none-manylinux_2_17_ppc64.manylinux2014_ppc64.whl", hash = "sha256:9de5d816da9a46f9119f0980f92ad275ec1a87c4e25818e7f3e8b46a503afe23", size = 12373961 },
            { url = "https://files.pythonhosted.org/packages/10/17/2ada97edbfd83968f59cc2f529973ad5ffddd930939c3bde846981fe90e2/uv-0.1.24-py3-none-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:f95f5224367186d4d4ee48795c7476a9ec21c48bcd141caf1d5cc5c9c811ab35", size = 11896951 },
            { url = "https://files.pythonhosted.org/packages/e2/d7/483cc6cc9ce15f8f105c47157c44796df6cf3ebdf96c8fa64486fae1aebd/uv-0.1.24-py3-none-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:e026831b555d219549275b52d64098c0b6f8bade8cc48c9b26dd88680f083743", size = 11913396 },
            { url = "https://files.pythonhosted.org/packages/04/89/b87ce99e550ad1b5d9311c5d88d2218733db10daa1402961cf9da064b3e8/uv-0.1.24-py3-none-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:94dfba0b9879e0141b9bcf6e6f990fe585f73082dbaac61d1ef3acd0dd013b9b", size = 11392629 },
            { url = "https://files.pythonhosted.org/packages/d8/28/a42463b004886a5793b750a0be1c1b7402cbdac06106cf1b5a109103ec13/uv-0.1.24-py3-none-manylinux_2_28_aarch64.whl", hash = "sha256:53bed670553025d7034592954454df5e31493d4e111c32a14c091669da0d38ba", size = 11100311 },
            { url = "https://files.pythonhosted.org/packages/ef/f1/ef3e5ff44c5b9da8f113971e56099fb630cd43230d69aa7a7b9f4bcfcd5c/uv-0.1.24-py3-none-musllinux_1_2_aarch64.whl", hash = "sha256:7ed69aa1798cb7f5000d4ac7d0b0185b39fd2798364a4f22f48ef97fd7e487e2", size = 10939800 },
            { url = "https://files.pythonhosted.org/packages/b1/fc/ca7acf7b986451af496fa21deda3306ff669f4d80070c1058a0f6eef4a6a/uv-0.1.24-py3-none-musllinux_1_2_armv7l.whl", hash = "sha256:850373ba442ee33328a89a83ceb053f3b53cb93d7505ff01b0a8575d9462f012", size = 9628356 },
            { url = "https://files.pythonhosted.org/packages/dd/bf/8cfe82bc9f0b2265a4c93ee2ed8b2e6f9948fa8ec9897845d7187f9bd858/uv-0.1.24-py3-none-musllinux_1_2_i686.whl", hash = "sha256:4d51afe18f1283417d2fccfc375a093c149a993ce25559982deeeb0b996c0c04", size = 10938717 },
            { url = "https://files.pythonhosted.org/packages/64/ec/ef00d07667ba492773a49131d0dcbb262716a3ea2f26cac4f029e775eec0/uv-0.1.24-py3-none-musllinux_1_2_x86_64.whl", hash = "sha256:a407f128672d1c4d924f3bf36fbba3df66d4d73a713811dd98aab5224528f201", size = 11492847 },
            { url = "https://files.pythonhosted.org/packages/c0/8b/a83689be89c764c98dd19be9e1a044e64ad9c1d17643c5c08262f640908f/uv-0.1.24-py3-none-win32.whl", hash = "sha256:a273f468f170b12e6cb6362aa90a65dd12a397db02df55ffb47a21c11b577d8c", size = 8143891 },
            { url = "https://files.pythonhosted.org/packages/d6/7d/55d81e3ecab3595ba9f7be7b394fec61d1e33c1852452e7997bc8022b6f0/uv-0.1.24-py3-none-win_amd64.whl", hash = "sha256:87cce6b44a86e761f845a85e52ea0de44fcd579b9a63b2d856118b0d1847bca7", size = 9178836 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Re-run with `--offline`. We shouldn't need a network connection to validate an
    // already-correct lockfile with immutable metadata.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    Ok(())
}

/// Correctly narrow the Python requirement when upper bounds are present.
///
/// See: <https://github.com/astral-sh/uv/issues/6911>
#[test]
fn lock_python_upper_bound() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.8"
        dependencies = ["magika; python_version < '3.13'"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 18 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.8"
        resolution-markers = [
            "python_full_version >= '3.9' and python_full_version < '3.13'",
            "python_full_version < '3.9'",
            "python_full_version >= '3.13'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "colorama", marker = "sys_platform == 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941 },
        ]

        [[package]]
        name = "colorama"
        version = "0.4.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d8/53/6f443c9a4a8358a93a6792e2acffb9d9d5cb0a5cfd8802644b7b1c9a02e4/colorama-0.4.6.tar.gz", hash = "sha256:08695f5cb7ed6e0531a20572697297273c47b8cae5a63ffc6d6ed5c201be6e44", size = 27697 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d1/d6/3965ed04c63042e047cb6a3e6ed1a63a35087b6a609aa3a15ed8ac56c221/colorama-0.4.6-py2.py3-none-any.whl", hash = "sha256:4f1d9991f5acc0ca119f9d443620b77f9d6b33703e51011c16baf57afb285fc6", size = 25335 },
        ]

        [[package]]
        name = "coloredlogs"
        version = "15.0.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "humanfriendly" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/cc/c7/eed8f27100517e8c0e6b923d5f0845d0cb99763da6fdee00478f91db7325/coloredlogs-15.0.1.tar.gz", hash = "sha256:7c991aa71a4577af2f82600d8f8f3a89f936baeaf9b50a9c197da014e5bf16b0", size = 278520 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a7/06/3d6badcf13db419e25b07041d9c7b4a2c331d3f4e7134445ec5df57714cd/coloredlogs-15.0.1-py2.py3-none-any.whl", hash = "sha256:612ee75c546f53e92e70049c9dbfcc18c935a2b9a53b66085ce9ef6a6e5c0934", size = 46018 },
        ]

        [[package]]
        name = "flatbuffers"
        version = "24.3.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/e7/75/b09ca56e5f0e0b04279a6b5ea756f578fe3d46be4a884a0e2fe322877351/flatbuffers-24.3.7.tar.gz", hash = "sha256:0895c22b9a6019ff2f4de2e5e2f7cd15914043e6e7033a94c0c6369422690f22", size = 22137 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/bf/45/c961e3cb6ddad76b325c163d730562bb6deb1ace5acbed0306f5fbefb90e/flatbuffers-24.3.7-py2.py3-none-any.whl", hash = "sha256:80c4f5dcad0ee76b7e349671a0d657f2fbba927a0244f88dd3f5ed6a3694e1fc", size = 26772 },
        ]

        [[package]]
        name = "humanfriendly"
        version = "10.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "pyreadline3", marker = "sys_platform == 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/cc/3f/2c29224acb2e2df4d2046e4c73ee2662023c58ff5b113c4c1adac0886c43/humanfriendly-10.0.tar.gz", hash = "sha256:6b0b831ce8f15f7300721aa49829fc4e83921a9a301cc7f606be6686a2288ddc", size = 360702 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f0/0f/310fb31e39e2d734ccaa2c0fb981ee41f7bd5056ce9bc29b2248bd569169/humanfriendly-10.0-py2.py3-none-any.whl", hash = "sha256:1697e1a8a8f550fd43c2865cd84542fc175a61dcb779b6fee18cf6b6ccba1477", size = 86794 },
        ]

        [[package]]
        name = "magika"
        version = "0.5.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "click" },
            { name = "numpy", version = "1.24.4", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version < '3.9'" },
            { name = "numpy", version = "1.26.4", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version >= '3.9' and python_full_version < '3.13'" },
            { name = "onnxruntime" },
            { name = "python-dotenv" },
            { name = "tabulate" },
            { name = "tqdm" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/1a/58/c1d8887354d0ff2256d4d78d08a69bcc55719a0189afa706c51da04390f2/magika-0.5.1.tar.gz", hash = "sha256:43dc1153a1637327225a626a1550c0a395a1d45ea33ec1f5d46b9b080238bee0", size = 1005077 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/66/79/e1c167ec35060692b70bfc4f2d0aa9314dd7e37ba8e30c1c27965e2f1daa/magika-0.5.1-py3-none-any.whl", hash = "sha256:a4d1f64f71460f335841c13c3d16cfc2cb21e839c1898a1ae9bd5adc8d66cb2b", size = 1008301 },
        ]

        [[package]]
        name = "mpmath"
        version = "1.3.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/e0/47/dd32fa426cc72114383ac549964eecb20ecfd886d1e5ccf5340b55b02f57/mpmath-1.3.0.tar.gz", hash = "sha256:7a28eb2a9774d00c7bc92411c19a89209d5da7c4c9a9e227be8330a23a25b91f", size = 508106 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/43/e3/7d92a15f894aa0c9c4b49b8ee9ac9850d6e63b03c9c32c0367a13ae62209/mpmath-1.3.0-py3-none-any.whl", hash = "sha256:a0b2b9fe80bbcd81a6647ff13108738cfb482d481d826cc0e02f5b35e5c88d2c", size = 536198 },
        ]

        [[package]]
        name = "numpy"
        version = "1.24.4"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version < '3.9'",
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/a4/9b/027bec52c633f6556dba6b722d9a0befb40498b9ceddd29cbe67a45a127c/numpy-1.24.4.tar.gz", hash = "sha256:80f5e3a4e498641401868df4208b74581206afbee7cf7b8329daae82676d9463", size = 10911229 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/6b/80/6cdfb3e275d95155a34659163b83c09e3a3ff9f1456880bec6cc63d71083/numpy-1.24.4-cp310-cp310-macosx_10_9_x86_64.whl", hash = "sha256:c0bfb52d2169d58c1cdb8cc1f16989101639b34c7d3ce60ed70b19c63eba0b64", size = 19789140 },
            { url = "https://files.pythonhosted.org/packages/64/5f/3f01d753e2175cfade1013eea08db99ba1ee4bdb147ebcf3623b75d12aa7/numpy-1.24.4-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:ed094d4f0c177b1b8e7aa9cba7d6ceed51c0e569a5318ac0ca9a090680a6a1b1", size = 13854297 },
            { url = "https://files.pythonhosted.org/packages/5a/b3/2f9c21d799fa07053ffa151faccdceeb69beec5a010576b8991f614021f7/numpy-1.24.4-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:79fc682a374c4a8ed08b331bef9c5f582585d1048fa6d80bc6c35bc384eee9b4", size = 13995611 },
            { url = "https://files.pythonhosted.org/packages/10/be/ae5bf4737cb79ba437879915791f6f26d92583c738d7d960ad94e5c36adf/numpy-1.24.4-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:7ffe43c74893dbf38c2b0a1f5428760a1a9c98285553c89e12d70a96a7f3a4d6", size = 17282357 },
            { url = "https://files.pythonhosted.org/packages/c0/64/908c1087be6285f40e4b3e79454552a701664a079321cff519d8c7051d06/numpy-1.24.4-cp310-cp310-win32.whl", hash = "sha256:4c21decb6ea94057331e111a5bed9a79d335658c27ce2adb580fb4d54f2ad9bc", size = 12429222 },
            { url = "https://files.pythonhosted.org/packages/22/55/3d5a7c1142e0d9329ad27cece17933b0e2ab4e54ddc5c1861fbfeb3f7693/numpy-1.24.4-cp310-cp310-win_amd64.whl", hash = "sha256:b4bea75e47d9586d31e892a7401f76e909712a0fd510f58f5337bea9572c571e", size = 14841514 },
            { url = "https://files.pythonhosted.org/packages/a9/cc/5ed2280a27e5dab12994c884f1f4d8c3bd4d885d02ae9e52a9d213a6a5e2/numpy-1.24.4-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:f136bab9c2cfd8da131132c2cf6cc27331dd6fae65f95f69dcd4ae3c3639c810", size = 19775508 },
            { url = "https://files.pythonhosted.org/packages/c0/bc/77635c657a3668cf652806210b8662e1aff84b818a55ba88257abf6637a8/numpy-1.24.4-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:e2926dac25b313635e4d6cf4dc4e51c8c0ebfed60b801c799ffc4c32bf3d1254", size = 13840033 },
            { url = "https://files.pythonhosted.org/packages/a7/4c/96cdaa34f54c05e97c1c50f39f98d608f96f0677a6589e64e53104e22904/numpy-1.24.4-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:222e40d0e2548690405b0b3c7b21d1169117391c2e82c378467ef9ab4c8f0da7", size = 13991951 },
            { url = "https://files.pythonhosted.org/packages/22/97/dfb1a31bb46686f09e68ea6ac5c63fdee0d22d7b23b8f3f7ea07712869ef/numpy-1.24.4-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:7215847ce88a85ce39baf9e89070cb860c98fdddacbaa6c0da3ffb31b3350bd5", size = 17278923 },
            { url = "https://files.pythonhosted.org/packages/35/e2/76a11e54139654a324d107da1d98f99e7aa2a7ef97cfd7c631fba7dbde71/numpy-1.24.4-cp311-cp311-win32.whl", hash = "sha256:4979217d7de511a8d57f4b4b5b2b965f707768440c17cb70fbf254c4b225238d", size = 12422446 },
            { url = "https://files.pythonhosted.org/packages/d8/ec/ebef2f7d7c28503f958f0f8b992e7ce606fb74f9e891199329d5f5f87404/numpy-1.24.4-cp311-cp311-win_amd64.whl", hash = "sha256:b7b1fc9864d7d39e28f41d089bfd6353cb5f27ecd9905348c24187a768c79694", size = 14834466 },
            { url = "https://files.pythonhosted.org/packages/11/10/943cfb579f1a02909ff96464c69893b1d25be3731b5d3652c2e0cf1281ea/numpy-1.24.4-cp38-cp38-macosx_10_9_x86_64.whl", hash = "sha256:1452241c290f3e2a312c137a9999cdbf63f78864d63c79039bda65ee86943f61", size = 19780722 },
            { url = "https://files.pythonhosted.org/packages/a7/ae/f53b7b265fdc701e663fbb322a8e9d4b14d9cb7b2385f45ddfabfc4327e4/numpy-1.24.4-cp38-cp38-macosx_11_0_arm64.whl", hash = "sha256:04640dab83f7c6c85abf9cd729c5b65f1ebd0ccf9de90b270cd61935eef0197f", size = 13843102 },
            { url = "https://files.pythonhosted.org/packages/25/6f/2586a50ad72e8dbb1d8381f837008a0321a3516dfd7cb57fc8cf7e4bb06b/numpy-1.24.4-cp38-cp38-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:a5425b114831d1e77e4b5d812b69d11d962e104095a5b9c3b641a218abcc050e", size = 14039616 },
            { url = "https://files.pythonhosted.org/packages/98/5d/5738903efe0ecb73e51eb44feafba32bdba2081263d40c5043568ff60faf/numpy-1.24.4-cp38-cp38-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:dd80e219fd4c71fc3699fc1dadac5dcf4fd882bfc6f7ec53d30fa197b8ee22dc", size = 17316263 },
            { url = "https://files.pythonhosted.org/packages/d1/57/8d328f0b91c733aa9aa7ee540dbc49b58796c862b4fbcb1146c701e888da/numpy-1.24.4-cp38-cp38-win32.whl", hash = "sha256:4602244f345453db537be5314d3983dbf5834a9701b7723ec28923e2889e0bb2", size = 12455660 },
            { url = "https://files.pythonhosted.org/packages/69/65/0d47953afa0ad569d12de5f65d964321c208492064c38fe3b0b9744f8d44/numpy-1.24.4-cp38-cp38-win_amd64.whl", hash = "sha256:692f2e0f55794943c5bfff12b3f56f99af76f902fc47487bdfe97856de51a706", size = 14868112 },
            { url = "https://files.pythonhosted.org/packages/9a/cd/d5b0402b801c8a8b56b04c1e85c6165efab298d2f0ab741c2406516ede3a/numpy-1.24.4-cp39-cp39-macosx_10_9_x86_64.whl", hash = "sha256:2541312fbf09977f3b3ad449c4e5f4bb55d0dbf79226d7724211acc905049400", size = 19816549 },
            { url = "https://files.pythonhosted.org/packages/14/27/638aaa446f39113a3ed38b37a66243e21b38110d021bfcb940c383e120f2/numpy-1.24.4-cp39-cp39-macosx_11_0_arm64.whl", hash = "sha256:9667575fb6d13c95f1b36aca12c5ee3356bf001b714fc354eb5465ce1609e62f", size = 13879950 },
            { url = "https://files.pythonhosted.org/packages/8f/27/91894916e50627476cff1a4e4363ab6179d01077d71b9afed41d9e1f18bf/numpy-1.24.4-cp39-cp39-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:f3a86ed21e4f87050382c7bc96571755193c4c1392490744ac73d660e8f564a9", size = 14030228 },
            { url = "https://files.pythonhosted.org/packages/7a/7c/d7b2a0417af6428440c0ad7cb9799073e507b1a465f827d058b826236964/numpy-1.24.4-cp39-cp39-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:d11efb4dbecbdf22508d55e48d9c8384db795e1b7b51ea735289ff96613ff74d", size = 17311170 },
            { url = "https://files.pythonhosted.org/packages/18/9d/e02ace5d7dfccee796c37b995c63322674daf88ae2f4a4724c5dd0afcc91/numpy-1.24.4-cp39-cp39-win32.whl", hash = "sha256:6620c0acd41dbcb368610bb2f4d83145674040025e5536954782467100aa8835", size = 12454918 },
            { url = "https://files.pythonhosted.org/packages/63/38/6cc19d6b8bfa1d1a459daf2b3fe325453153ca7019976274b6f33d8b5663/numpy-1.24.4-cp39-cp39-win_amd64.whl", hash = "sha256:befe2bf740fd8373cf56149a5c23a0f601e82869598d41f8e188a0e9869926f8", size = 14867441 },
            { url = "https://files.pythonhosted.org/packages/a4/fd/8dff40e25e937c94257455c237b9b6bf5a30d42dd1cc11555533be099492/numpy-1.24.4-pp38-pypy38_pp73-macosx_10_9_x86_64.whl", hash = "sha256:31f13e25b4e304632a4619d0e0777662c2ffea99fcae2029556b17d8ff958aef", size = 19156590 },
            { url = "https://files.pythonhosted.org/packages/42/e7/4bf953c6e05df90c6d351af69966384fed8e988d0e8c54dad7103b59f3ba/numpy-1.24.4-pp38-pypy38_pp73-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:95f7ac6540e95bc440ad77f56e520da5bf877f87dca58bd095288dce8940532a", size = 16705744 },
            { url = "https://files.pythonhosted.org/packages/fc/dd/9106005eb477d022b60b3817ed5937a43dad8fd1f20b0610ea8a32fcb407/numpy-1.24.4-pp38-pypy38_pp73-win_amd64.whl", hash = "sha256:e98f220aa76ca2a977fe435f5b04d7b3470c0a2e6312907b37ba6068f26787f2", size = 14734290 },
        ]

        [[package]]
        name = "numpy"
        version = "1.26.4"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.9' and python_full_version < '3.13'",
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/65/6e/09db70a523a96d25e115e71cc56a6f9031e7b8cd166c1ac8438307c14058/numpy-1.26.4.tar.gz", hash = "sha256:2a02aba9ed12e4ac4eb3ea9421c420301a0c6460d9830d74a9df87efa4912010", size = 15786129 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a7/94/ace0fdea5241a27d13543ee117cbc65868e82213fb31a8eb7fe9ff23f313/numpy-1.26.4-cp310-cp310-macosx_10_9_x86_64.whl", hash = "sha256:9ff0f4f29c51e2803569d7a51c2304de5554655a60c5d776e35b4a41413830d0", size = 20631468 },
            { url = "https://files.pythonhosted.org/packages/20/f7/b24208eba89f9d1b58c1668bc6c8c4fd472b20c45573cb767f59d49fb0f6/numpy-1.26.4-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:2e4ee3380d6de9c9ec04745830fd9e2eccb3e6cf790d39d7b98ffd19b0dd754a", size = 13966411 },
            { url = "https://files.pythonhosted.org/packages/fc/a5/4beee6488160798683eed5bdb7eead455892c3b4e1f78d79d8d3f3b084ac/numpy-1.26.4-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:d209d8969599b27ad20994c8e41936ee0964e6da07478d6c35016bc386b66ad4", size = 14219016 },
            { url = "https://files.pythonhosted.org/packages/4b/d7/ecf66c1cd12dc28b4040b15ab4d17b773b87fa9d29ca16125de01adb36cd/numpy-1.26.4-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:ffa75af20b44f8dba823498024771d5ac50620e6915abac414251bd971b4529f", size = 18240889 },
            { url = "https://files.pythonhosted.org/packages/24/03/6f229fe3187546435c4f6f89f6d26c129d4f5bed40552899fcf1f0bf9e50/numpy-1.26.4-cp310-cp310-musllinux_1_1_aarch64.whl", hash = "sha256:62b8e4b1e28009ef2846b4c7852046736bab361f7aeadeb6a5b89ebec3c7055a", size = 13876746 },
            { url = "https://files.pythonhosted.org/packages/39/fe/39ada9b094f01f5a35486577c848fe274e374bbf8d8f472e1423a0bbd26d/numpy-1.26.4-cp310-cp310-musllinux_1_1_x86_64.whl", hash = "sha256:a4abb4f9001ad2858e7ac189089c42178fcce737e4169dc61321660f1a96c7d2", size = 18078620 },
            { url = "https://files.pythonhosted.org/packages/d5/ef/6ad11d51197aad206a9ad2286dc1aac6a378059e06e8cf22cd08ed4f20dc/numpy-1.26.4-cp310-cp310-win32.whl", hash = "sha256:bfe25acf8b437eb2a8b2d49d443800a5f18508cd811fea3181723922a8a82b07", size = 5972659 },
            { url = "https://files.pythonhosted.org/packages/19/77/538f202862b9183f54108557bfda67e17603fc560c384559e769321c9d92/numpy-1.26.4-cp310-cp310-win_amd64.whl", hash = "sha256:b97fe8060236edf3662adfc2c633f56a08ae30560c56310562cb4f95500022d5", size = 15808905 },
            { url = "https://files.pythonhosted.org/packages/11/57/baae43d14fe163fa0e4c47f307b6b2511ab8d7d30177c491960504252053/numpy-1.26.4-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:4c66707fabe114439db9068ee468c26bbdf909cac0fb58686a42a24de1760c71", size = 20630554 },
            { url = "https://files.pythonhosted.org/packages/1a/2e/151484f49fd03944c4a3ad9c418ed193cfd02724e138ac8a9505d056c582/numpy-1.26.4-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:edd8b5fe47dab091176d21bb6de568acdd906d1887a4584a15a9a96a1dca06ef", size = 13997127 },
            { url = "https://files.pythonhosted.org/packages/79/ae/7e5b85136806f9dadf4878bf73cf223fe5c2636818ba3ab1c585d0403164/numpy-1.26.4-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:7ab55401287bfec946ced39700c053796e7cc0e3acbef09993a9ad2adba6ca6e", size = 14222994 },
            { url = "https://files.pythonhosted.org/packages/3a/d0/edc009c27b406c4f9cbc79274d6e46d634d139075492ad055e3d68445925/numpy-1.26.4-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:666dbfb6ec68962c033a450943ded891bed2d54e6755e35e5835d63f4f6931d5", size = 18252005 },
            { url = "https://files.pythonhosted.org/packages/09/bf/2b1aaf8f525f2923ff6cfcf134ae5e750e279ac65ebf386c75a0cf6da06a/numpy-1.26.4-cp311-cp311-musllinux_1_1_aarch64.whl", hash = "sha256:96ff0b2ad353d8f990b63294c8986f1ec3cb19d749234014f4e7eb0112ceba5a", size = 13885297 },
            { url = "https://files.pythonhosted.org/packages/df/a0/4e0f14d847cfc2a633a1c8621d00724f3206cfeddeb66d35698c4e2cf3d2/numpy-1.26.4-cp311-cp311-musllinux_1_1_x86_64.whl", hash = "sha256:60dedbb91afcbfdc9bc0b1f3f402804070deed7392c23eb7a7f07fa857868e8a", size = 18093567 },
            { url = "https://files.pythonhosted.org/packages/d2/b7/a734c733286e10a7f1a8ad1ae8c90f2d33bf604a96548e0a4a3a6739b468/numpy-1.26.4-cp311-cp311-win32.whl", hash = "sha256:1af303d6b2210eb850fcf03064d364652b7120803a0b872f5211f5234b399f20", size = 5968812 },
            { url = "https://files.pythonhosted.org/packages/3f/6b/5610004206cf7f8e7ad91c5a85a8c71b2f2f8051a0c0c4d5916b76d6cbb2/numpy-1.26.4-cp311-cp311-win_amd64.whl", hash = "sha256:cd25bcecc4974d09257ffcd1f098ee778f7834c3ad767fe5db785be9a4aa9cb2", size = 15811913 },
            { url = "https://files.pythonhosted.org/packages/95/12/8f2020a8e8b8383ac0177dc9570aad031a3beb12e38847f7129bacd96228/numpy-1.26.4-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:b3ce300f3644fb06443ee2222c2201dd3a89ea6040541412b8fa189341847218", size = 20335901 },
            { url = "https://files.pythonhosted.org/packages/75/5b/ca6c8bd14007e5ca171c7c03102d17b4f4e0ceb53957e8c44343a9546dcc/numpy-1.26.4-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:03a8c78d01d9781b28a6989f6fa1bb2c4f2d51201cf99d3dd875df6fbd96b23b", size = 13685868 },
            { url = "https://files.pythonhosted.org/packages/79/f8/97f10e6755e2a7d027ca783f63044d5b1bc1ae7acb12afe6a9b4286eac17/numpy-1.26.4-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:9fad7dcb1aac3c7f0584a5a8133e3a43eeb2fe127f47e3632d43d677c66c102b", size = 13925109 },
            { url = "https://files.pythonhosted.org/packages/0f/50/de23fde84e45f5c4fda2488c759b69990fd4512387a8632860f3ac9cd225/numpy-1.26.4-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:675d61ffbfa78604709862923189bad94014bef562cc35cf61d3a07bba02a7ed", size = 17950613 },
            { url = "https://files.pythonhosted.org/packages/4c/0c/9c603826b6465e82591e05ca230dfc13376da512b25ccd0894709b054ed0/numpy-1.26.4-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:ab47dbe5cc8210f55aa58e4805fe224dac469cde56b9f731a4c098b91917159a", size = 13572172 },
            { url = "https://files.pythonhosted.org/packages/76/8c/2ba3902e1a0fc1c74962ea9bb33a534bb05984ad7ff9515bf8d07527cadd/numpy-1.26.4-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:1dda2e7b4ec9dd512f84935c5f126c8bd8b9f2fc001e9f54af255e8c5f16b0e0", size = 17786643 },
            { url = "https://files.pythonhosted.org/packages/28/4a/46d9e65106879492374999e76eb85f87b15328e06bd1550668f79f7b18c6/numpy-1.26.4-cp312-cp312-win32.whl", hash = "sha256:50193e430acfc1346175fcbdaa28ffec49947a06918b7b92130744e81e640110", size = 5677803 },
            { url = "https://files.pythonhosted.org/packages/16/2e/86f24451c2d530c88daf997cb8d6ac622c1d40d19f5a031ed68a4b73a374/numpy-1.26.4-cp312-cp312-win_amd64.whl", hash = "sha256:08beddf13648eb95f8d867350f6a018a4be2e5ad54c8d8caed89ebca558b2818", size = 15517754 },
            { url = "https://files.pythonhosted.org/packages/7d/24/ce71dc08f06534269f66e73c04f5709ee024a1afe92a7b6e1d73f158e1f8/numpy-1.26.4-cp39-cp39-macosx_10_9_x86_64.whl", hash = "sha256:7349ab0fa0c429c82442a27a9673fc802ffdb7c7775fad780226cb234965e53c", size = 20636301 },
            { url = "https://files.pythonhosted.org/packages/ae/8c/ab03a7c25741f9ebc92684a20125fbc9fc1b8e1e700beb9197d750fdff88/numpy-1.26.4-cp39-cp39-macosx_11_0_arm64.whl", hash = "sha256:52b8b60467cd7dd1e9ed082188b4e6bb35aa5cdd01777621a1658910745b90be", size = 13971216 },
            { url = "https://files.pythonhosted.org/packages/6d/64/c3bcdf822269421d85fe0d64ba972003f9bb4aa9a419da64b86856c9961f/numpy-1.26.4-cp39-cp39-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:d5241e0a80d808d70546c697135da2c613f30e28251ff8307eb72ba696945764", size = 14226281 },
            { url = "https://files.pythonhosted.org/packages/54/30/c2a907b9443cf42b90c17ad10c1e8fa801975f01cb9764f3f8eb8aea638b/numpy-1.26.4-cp39-cp39-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f870204a840a60da0b12273ef34f7051e98c3b5961b61b0c2c1be6dfd64fbcd3", size = 18249516 },
            { url = "https://files.pythonhosted.org/packages/43/12/01a563fc44c07095996d0129b8899daf89e4742146f7044cdbdb3101c57f/numpy-1.26.4-cp39-cp39-musllinux_1_1_aarch64.whl", hash = "sha256:679b0076f67ecc0138fd2ede3a8fd196dddc2ad3254069bcb9faf9a79b1cebcd", size = 13882132 },
            { url = "https://files.pythonhosted.org/packages/16/ee/9df80b06680aaa23fc6c31211387e0db349e0e36d6a63ba3bd78c5acdf11/numpy-1.26.4-cp39-cp39-musllinux_1_1_x86_64.whl", hash = "sha256:47711010ad8555514b434df65f7d7b076bb8261df1ca9bb78f53d3b2db02e95c", size = 18084181 },
            { url = "https://files.pythonhosted.org/packages/28/7d/4b92e2fe20b214ffca36107f1a3e75ef4c488430e64de2d9af5db3a4637d/numpy-1.26.4-cp39-cp39-win32.whl", hash = "sha256:a354325ee03388678242a4d7ebcd08b5c727033fcff3b2f536aea978e15ee9e6", size = 5976360 },
            { url = "https://files.pythonhosted.org/packages/b5/42/054082bd8220bbf6f297f982f0a8f5479fcbc55c8b511d928df07b965869/numpy-1.26.4-cp39-cp39-win_amd64.whl", hash = "sha256:3373d5d70a5fe74a2c1bb6d2cfd9609ecf686d47a2d7b1d37a8f3b6bf6003aea", size = 15814633 },
            { url = "https://files.pythonhosted.org/packages/3f/72/3df6c1c06fc83d9cfe381cccb4be2532bbd38bf93fbc9fad087b6687f1c0/numpy-1.26.4-pp39-pypy39_pp73-macosx_10_9_x86_64.whl", hash = "sha256:afedb719a9dcfc7eaf2287b839d8198e06dcd4cb5d276a3df279231138e83d30", size = 20455961 },
            { url = "https://files.pythonhosted.org/packages/8e/02/570545bac308b58ffb21adda0f4e220ba716fb658a63c151daecc3293350/numpy-1.26.4-pp39-pypy39_pp73-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:95a7476c59002f2f6c590b9b7b998306fba6a5aa646b1e22ddfeaf8f78c3a29c", size = 18061071 },
            { url = "https://files.pythonhosted.org/packages/f4/5f/fafd8c51235f60d49f7a88e2275e13971e90555b67da52dd6416caec32fe/numpy-1.26.4-pp39-pypy39_pp73-win_amd64.whl", hash = "sha256:7e50d0a0cc3189f9cb0aeb3a6a6af18c16f59f004b866cd2be1c14b36134a4a0", size = 15709730 },
        ]

        [[package]]
        name = "onnxruntime"
        version = "1.17.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "coloredlogs" },
            { name = "flatbuffers" },
            { name = "numpy", version = "1.24.4", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version < '3.9'" },
            { name = "numpy", version = "1.26.4", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version >= '3.9'" },
            { name = "packaging" },
            { name = "protobuf" },
            { name = "sympy" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/8f/59/e10881d48f6f132eb6135dad27079a2fcb8b41861d7b33556eb3d7bfcecb/onnxruntime-1.17.1-cp310-cp310-macosx_11_0_universal2.whl", hash = "sha256:d43ac17ac4fa3c9096ad3c0e5255bb41fd134560212dc124e7f52c3159af5d21", size = 14762759 },
            { url = "https://files.pythonhosted.org/packages/9d/d9/fd6b7156b657bf921a0b09c26bfe63848805ad674059912c5d33bc99e56b/onnxruntime-1.17.1-cp310-cp310-manylinux_2_27_aarch64.manylinux_2_28_aarch64.whl", hash = "sha256:55b5e92a4c76a23981c998078b9bf6145e4fb0b016321a8274b1607bd3c6bd35", size = 5857784 },
            { url = "https://files.pythonhosted.org/packages/3d/85/db9e2ccef45f1670013495084bf74db7b24e2edfd3d7be92211eb968e369/onnxruntime-1.17.1-cp310-cp310-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl", hash = "sha256:ebbcd2bc3a066cf54e6f18c75708eb4d309ef42be54606d22e5bdd78afc5b0d7", size = 6786777 },
            { url = "https://files.pythonhosted.org/packages/69/2c/75c014cfe4120faa14f4387faf40ffe32e84ec47f07b48e0662e130ef2f3/onnxruntime-1.17.1-cp310-cp310-win32.whl", hash = "sha256:5e3716b5eec9092e29a8d17aab55e737480487deabfca7eac3cd3ed952b6ada9", size = 4934432 },
            { url = "https://files.pythonhosted.org/packages/e0/fa/f5b8a7dc6c0b7d059c073dcb192bfb02c86a6006c8735509eb59b9ea11f0/onnxruntime-1.17.1-cp310-cp310-win_amd64.whl", hash = "sha256:fbb98cced6782ae1bb799cc74ddcbbeeae8819f3ad1d942a74d88e72b6511337", size = 5599711 },
            { url = "https://files.pythonhosted.org/packages/89/82/4744bcdefd82e910d530d9f220fd8583d883e2275b6828bc25876051b28c/onnxruntime-1.17.1-cp311-cp311-macosx_11_0_universal2.whl", hash = "sha256:36fd6f87a1ecad87e9c652e42407a50fb305374f9a31d71293eb231caae18784", size = 14762988 },
            { url = "https://files.pythonhosted.org/packages/cd/a7/0cf482a4c6b7da6f551f3d80b015070665951d880f422221a345207ce680/onnxruntime-1.17.1-cp311-cp311-manylinux_2_27_aarch64.manylinux_2_28_aarch64.whl", hash = "sha256:99a8bddeb538edabc524d468edb60ad4722cff8a49d66f4e280c39eace70500b", size = 5851009 },
            { url = "https://files.pythonhosted.org/packages/fb/32/713dc08e5bd4a9e70e5f5c621d4dbe2f9c039fbd37ec6b94956485e0f90d/onnxruntime-1.17.1-cp311-cp311-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl", hash = "sha256:fd7fddb4311deb5a7d3390cd8e9b3912d4d963efbe4dfe075edbaf18d01c024e", size = 6787425 },
            { url = "https://files.pythonhosted.org/packages/a0/dc/b8ceb8bd4f9d3375c5d0771dfa2edb773cde06243bafa62e57635e891e1e/onnxruntime-1.17.1-cp311-cp311-win32.whl", hash = "sha256:606a7cbfb6680202b0e4f1890881041ffc3ac6e41760a25763bd9fe146f0b335", size = 4934458 },
            { url = "https://files.pythonhosted.org/packages/43/24/37ca26db9e135817cbda65bae34af02a6f5c394a3968d07f2bf190e788fd/onnxruntime-1.17.1-cp311-cp311-win_amd64.whl", hash = "sha256:53e4e06c0a541696ebdf96085fd9390304b7b04b748a19e02cf3b35c869a1e76", size = 5599784 },
            { url = "https://files.pythonhosted.org/packages/a1/a2/00ea929ccf9b4702af11e13fa52e9ac607aecc867b041ffcbe646e29f880/onnxruntime-1.17.1-cp312-cp312-macosx_11_0_universal2.whl", hash = "sha256:40f08e378e0f85929712a2b2c9b9a9cc400a90c8a8ca741d1d92c00abec60843", size = 14774341 },
            { url = "https://files.pythonhosted.org/packages/36/82/764e94ca19ca5b694aeb777191d5973189d67e303bdf68e9df2147af267f/onnxruntime-1.17.1-cp312-cp312-manylinux_2_27_aarch64.manylinux_2_28_aarch64.whl", hash = "sha256:ac79da6d3e1bb4590f1dad4bb3c2979d7228555f92bb39820889af8b8e6bd472", size = 5849157 },
            { url = "https://files.pythonhosted.org/packages/6f/75/f22390f6d216c52639400bb5e7d7f96d0c5445de5ca18f48dba17669e4e1/onnxruntime-1.17.1-cp312-cp312-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl", hash = "sha256:ae9ba47dc099004e3781f2d0814ad710a13c868c739ab086fc697524061695ea", size = 6790530 },
            { url = "https://files.pythonhosted.org/packages/fa/a0/d24a8382c6a0d596690a06e981aef5a75a03bfc7cdc34f35fe7eb0a93988/onnxruntime-1.17.1-cp312-cp312-win32.whl", hash = "sha256:2dff1a24354220ac30e4a4ce2fb1df38cb1ea59f7dac2c116238d63fe7f4c5ff", size = 4935150 },
            { url = "https://files.pythonhosted.org/packages/81/e5/55699490257383a0a579b43eeea545680edc8f956f83fde1e16c5fd2359e/onnxruntime-1.17.1-cp312-cp312-win_amd64.whl", hash = "sha256:6226a5201ab8cafb15e12e72ff2a4fc8f50654e8fa5737c6f0bd57c5ff66827e", size = 5602101 },
            { url = "https://files.pythonhosted.org/packages/22/46/54e5f5dec8d024bd077a3b48c9462e9310e03c5e1bf6104c3ecf683e2816/onnxruntime-1.17.1-cp38-cp38-macosx_11_0_universal2.whl", hash = "sha256:cd0c07c0d1dfb8629e820b05fda5739e4835b3b82faf43753d2998edf2cf00aa", size = 14762572 },
            { url = "https://files.pythonhosted.org/packages/82/c1/8091fefeaa5bcc0d9289008da5a9d261a5b6d31385dca4ae794b026f1dc4/onnxruntime-1.17.1-cp38-cp38-manylinux_2_27_aarch64.manylinux_2_28_aarch64.whl", hash = "sha256:617ebdf49184efa1ba6e4467e602fbfa029ed52c92f13ce3c9f417d303006381", size = 5849663 },
            { url = "https://files.pythonhosted.org/packages/6b/14/ae3f3af6838c89583797bf5b39595ecadafa1ab00856138fe3d583d06a5d/onnxruntime-1.17.1-cp38-cp38-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl", hash = "sha256:9dae9071e3facdf2920769dceee03b71c684b6439021defa45b830d05e148924", size = 6788155 },
            { url = "https://files.pythonhosted.org/packages/5b/00/e3946a582f95ee58202129f5e12a509a0859d16d0aebd126c872c31e4fa0/onnxruntime-1.17.1-cp38-cp38-win32.whl", hash = "sha256:835d38fa1064841679433b1aa8138b5e1218ddf0cfa7a3ae0d056d8fd9cec713", size = 4934004 },
            { url = "https://files.pythonhosted.org/packages/57/41/f57ab2750e022ccb7c59d82515c0387cf8bea57f6c0df2c7c194f5244eda/onnxruntime-1.17.1-cp38-cp38-win_amd64.whl", hash = "sha256:96621e0c555c2453bf607606d08af3f70fbf6f315230c28ddea91754e17ad4e6", size = 5599720 },
            { url = "https://files.pythonhosted.org/packages/72/a8/737698c67038b8c2c8c2d7465988d0c84e884fe88fe17f116c969034a877/onnxruntime-1.17.1-cp39-cp39-macosx_11_0_universal2.whl", hash = "sha256:7a9539935fb2d78ebf2cf2693cad02d9930b0fb23cdd5cf37a7df813e977674d", size = 14762505 },
            { url = "https://files.pythonhosted.org/packages/02/a2/69bdf1c743811eca82f5f583a96228d62f5c0a90a6013b3f22da65bd96af/onnxruntime-1.17.1-cp39-cp39-manylinux_2_27_aarch64.manylinux_2_28_aarch64.whl", hash = "sha256:45c6a384e9d9a29c78afff62032a46a993c477b280247a7e335df09372aedbe9", size = 5853144 },
            { url = "https://files.pythonhosted.org/packages/6c/84/369acbaa48c416b40c3fd3b2a4ff41846d20190bbb06042a0ccd9afba03f/onnxruntime-1.17.1-cp39-cp39-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl", hash = "sha256:4e19f966450f16863a1d6182a685ca33ae04d7772a76132303852d05b95411ea", size = 6787843 },
            { url = "https://files.pythonhosted.org/packages/cd/20/543282495aaeaf730a10e8901a8bc69805b55045797efd7006d12c197ce8/onnxruntime-1.17.1-cp39-cp39-win32.whl", hash = "sha256:e2ae712d64a42aac29ed7a40a426cb1e624a08cfe9273dcfe681614aa65b07dc", size = 4934462 },
            { url = "https://files.pythonhosted.org/packages/cf/7f/f11990c263bbc144e1fcbba1061be5bcff7f09494dbf3186d2c5a8a17835/onnxruntime-1.17.1-cp39-cp39-win_amd64.whl", hash = "sha256:f7e9f7fb049825cdddf4a923cfc7c649d84d63c0134315f8e0aa9e0c3004672c", size = 5601956 },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "magika", marker = "python_full_version < '3.13'" },
        ]

        [package.metadata]
        requires-dist = [{ name = "magika", marker = "python_full_version < '3.13'" }]

        [[package]]
        name = "protobuf"
        version = "5.26.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ea/ab/ae590cd71f5a50cd9e0979593e217529b532a001e46c2dd0811c8697f4ad/protobuf-5.26.0.tar.gz", hash = "sha256:82f5870d74c99addfe4152777bdf8168244b9cf0ac65f8eccf045ddfa9d80d9b", size = 393498 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/91/b1/bd8403bf96aae15e814532f14b12c1e7a1acad636876d470e1888ff51c59/protobuf-5.26.0-cp310-abi3-win32.whl", hash = "sha256:f9ecc8eb6f18037e0cbf43256db0325d4723f429bca7ef5cd358b7c29d65f628", size = 400024 },
            { url = "https://files.pythonhosted.org/packages/cf/20/576e9d592c5d529c571bf6c61801f761d0488bbfb2e8a5c760a784c9ddef/protobuf-5.26.0-cp310-abi3-win_amd64.whl", hash = "sha256:dfd29f6eb34107dccf289a93d44fb6b131e68888d090b784b691775ac84e8213", size = 420905 },
            { url = "https://files.pythonhosted.org/packages/7d/98/282bfe05071c9e1e7e4af692bc937e12e7adacf3b297e0274579de01c9d7/protobuf-5.26.0-cp37-abi3-macosx_10_9_universal2.whl", hash = "sha256:7e47c57303466c867374a17b2b5e99c5a7c8b72a94118e2f28efb599f19b4069", size = 404047 },
            { url = "https://files.pythonhosted.org/packages/69/fd/0afea50450851849d60b8133abc887ce40a71e6098c8ea5cf7bf05b28b20/protobuf-5.26.0-cp37-abi3-manylinux2014_aarch64.whl", hash = "sha256:e184175276edc222e2d5e314a72521e10049938a9a4961fe4bea9b25d073c03f", size = 300881 },
            { url = "https://files.pythonhosted.org/packages/39/3f/16bdd9d43b024c1d178e817826e4e1ca8a25da3faff1e7566f341094143d/protobuf-5.26.0-cp37-abi3-manylinux2014_x86_64.whl", hash = "sha256:6ee9d1aa02f951c5ce10bf8c6cfb7604133773038e33f913183c8b5201350600", size = 302825 },
            { url = "https://files.pythonhosted.org/packages/bc/b7/50594140df80d9934829b1c873a86dcf0ce4c8f922e53e8758bf3d7f7040/protobuf-5.26.0-cp38-cp38-win32.whl", hash = "sha256:2c334550e1cb4efac5c8a3987384bf13a4334abaf5ab59e40479e7b70ecd6b19", size = 399995 },
            { url = "https://files.pythonhosted.org/packages/89/5e/7f05735dd9772ded7e5d60301baa58d53a54c90a67ca0d1a18e86f3ec8cf/protobuf-5.26.0-cp38-cp38-win_amd64.whl", hash = "sha256:8eef61a90631c21b06b4f492a27e199a269827f046de3bb68b61aa84fcf50905", size = 420907 },
            { url = "https://files.pythonhosted.org/packages/db/4d/6b844b343dbd630f4090d0d415327eaa7d93a1a5ea646d228e0c784bbe59/protobuf-5.26.0-cp39-cp39-win32.whl", hash = "sha256:ca825f4eecb8c342d2ec581e6a5ad1ad1a47bededaecd768e0d3451ae4aaac2b", size = 400055 },
            { url = "https://files.pythonhosted.org/packages/38/52/85a3af7d48e010aca1971cde510fb1e315bf23d0bce6c6cd7998a2f9841f/protobuf-5.26.0-cp39-cp39-win_amd64.whl", hash = "sha256:efd4f5894c50bd76cbcfdd668cd941021333861ed0f441c78a83d8254a01cc9f", size = 420997 },
            { url = "https://files.pythonhosted.org/packages/82/98/757626ed06e0d99c59428f5761c57c0e35bddb45041c0e222183492d6ddb/protobuf-5.26.0-py3-none-any.whl", hash = "sha256:a49b6c5359bf34fb7bf965bf21abfab4476e4527d822ab5289ee3bf73f291159", size = 161231 },
        ]

        [[package]]
        name = "pyreadline3"
        version = "3.4.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/86/3d61a61f36a0067874a00cb4dceb9028d34b6060e47828f7fc86fb9f7ee9/pyreadline3-3.4.1.tar.gz", hash = "sha256:6f3d1f7b8a31ba32b73917cefc1f28cc660562f39aea8646d30bd6eff21f7bae", size = 86465 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/56/fc/a3c13ded7b3057680c8ae95a9b6cc83e63657c38e0005c400a5d018a33a7/pyreadline3-3.4.1-py3-none-any.whl", hash = "sha256:b0efb6516fd4fb07b45949053826a62fa4cb353db5be2bbb4a7aa1fdd1e345fb", size = 95203 },
        ]

        [[package]]
        name = "python-dotenv"
        version = "1.0.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bc/57/e84d88dfe0aec03b7a2d4327012c1627ab5f03652216c63d49846d7a6c58/python-dotenv-1.0.1.tar.gz", hash = "sha256:e324ee90a023d808f1959c46bcbc04446a10ced277783dc6ee09987c37ec10ca", size = 39115 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/6a/3e/b68c118422ec867fa7ab88444e1274aa40681c606d59ac27de5a5588f082/python_dotenv-1.0.1-py3-none-any.whl", hash = "sha256:f7b63ef50f1b690dddf550d03497b66d609393b40b564ed0d674909a68ebf16a", size = 19863 },
        ]

        [[package]]
        name = "sympy"
        version = "1.12"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "mpmath" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/e5/57/3485a1a3dff51bfd691962768b14310dae452431754bfc091250be50dd29/sympy-1.12.tar.gz", hash = "sha256:ebf595c8dac3e0fdc4152c51878b498396ec7f30e7a914d6071e674d49420fb8", size = 6722203 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d2/05/e6600db80270777c4a64238a98d442f0fd07cc8915be2a1c16da7f2b9e74/sympy-1.12-py3-none-any.whl", hash = "sha256:c3588cd4295d0c0f603d0f2ae780587e64e2efeedb3521e46b9bb1d08d184fa5", size = 5742435 },
        ]

        [[package]]
        name = "tabulate"
        version = "0.9.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ec/fe/802052aecb21e3797b8f7902564ab6ea0d60ff8ca23952079064155d1ae1/tabulate-0.9.0.tar.gz", hash = "sha256:0095b12bf5966de529c0feb1fa08671671b3368eec77d7ef7ab114be2c068b3c", size = 81090 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/40/44/4a5f08c96eb108af5cb50b41f76142f0afa346dfa99d5296fe7202a11854/tabulate-0.9.0-py3-none-any.whl", hash = "sha256:024ca478df22e9340661486f85298cff5f6dcdba14f3813e8830015b9ed1948f", size = 35252 },
        ]

        [[package]]
        name = "tqdm"
        version = "4.66.2"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "colorama", marker = "sys_platform == 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/ea/85/3ce0f9f7d3f596e7ea57f4e5ce8c18cb44e4a9daa58ddb46ee0d13d6bff8/tqdm-4.66.2.tar.gz", hash = "sha256:6cd52cdf0fef0e0f543299cfc96fec90d7b8a7e88745f411ec33eb44d5ed3531", size = 169462 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/14/e75e52d521442e2fcc9f1df3c5e456aead034203d4797867980de558ab34/tqdm-4.66.2-py3-none-any.whl", hash = "sha256:1ee4f8a893eb9bef51c6e35730cebf234d5d0b6bd112b0271e10ed7c24a02bd9", size = 78296 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 18 packages in [TIME]
    "###);

    // Re-run with `--offline`. We shouldn't need a network connection to validate an
    // already-correct lockfile with immutable metadata.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 18 packages in [TIME]
    "###);

    Ok(())
}

/// See: <https://github.com/astral-sh/uv/issues/7876>
#[test]
fn lock_simplified_environments() -> Result<()> {
    let context = TestContext::new("3.11");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.11,<3.12"
        dependencies = ["iniconfig"]

        [tool.uv]
        environments = [
            "sys_platform == 'darwin' and python_version >= '3.11' and python_version < '3.12'",
            "sys_platform != 'darwin' and python_version >= '3.11' and python_version < '3.12'",
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = "==3.11.*"
        resolution-markers = [
            "sys_platform == 'darwin'",
            "sys_platform != 'darwin'",
        ]
        supported-markers = [
            "sys_platform == 'darwin'",
            "sys_platform != 'darwin'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig" }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Re-run with `--offline`. We shouldn't need a network connection to validate an
    // already-correct lockfile with immutable metadata.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    "###);

    Ok(())
}

#[test]
fn lock_dependency_metadata() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [[tool.uv.dependency-metadata]]
        name = "anyio"
        version = "3.7.0"
        requires-dist = ["iniconfig"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]

        [[manifest.dependency-metadata]]
        name = "anyio"
        version = "3.7.0"
        requires-dist = ["iniconfig"]

        [[package]]
        name = "anyio"
        version = "3.7.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "iniconfig" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/c6/b3/fefbf7e78ab3b805dec67d698dc18dd505af7a18a8dd08868c9b4fa736b5/anyio-3.7.0.tar.gz", hash = "sha256:275d9973793619a5374e1c89a4f4ad3f4b0a5510a2b5b939444bee8f4c4d37ce", size = 142737 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/fe/7ce1926952c8a403b35029e194555558514b365ad77d75125f521a2bec62/anyio-3.7.0-py3-none-any.whl", hash = "sha256:eddca883c4175f14df8aedce21054bfca3adb70ffe76a9f607aef9d7fa2ea7f0", size = 80873 },
        ]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio", specifier = "==3.7.0" }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    // Re-run with `--offline`. We shouldn't need a network connection to validate an
    // already-correct lockfile with immutable metadata.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==3.7.0
     + iniconfig==2.0.0
     + project==0.1.0 (from file://[TEMP_DIR]/)
    "###);

    // Update the static metadata.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [[tool.uv.dependency-metadata]]
        name = "anyio"
        version = "3.7.0"
        requires-dist = ["typing-extensions"]
        "#,
    )?;

    // The lockfile should update.
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Removed iniconfig v2.0.0
    Added typing-extensions v4.10.0
    "###);

    // Remove the static metadata.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    // The lockfile should update.
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Added idna v3.6
    Added sniffio v1.3.1
    Removed typing-extensions v4.10.0
    "###);

    // Use a blanket match.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [[tool.uv.dependency-metadata]]
        name = "anyio"
        requires-dist = ["iniconfig"]
        "#,
    )?;

    // The lockfile should update.
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Removed idna v3.6
    Added iniconfig v2.0.0
    Removed sniffio v1.3.1
    "###);

    Ok(())
}

#[test]
fn lock_dependency_metadata_git() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio"]

        [tool.uv.sources]
        anyio = { git = "https://github.com/agronholm/anyio", tag = "4.6.2" }

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [[tool.uv.dependency-metadata]]
        name = "anyio"
        version = "4.6.0.post2"
        requires-dist = ["iniconfig"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]

        [[manifest.dependency-metadata]]
        name = "anyio"
        version = "4.6.0.post2"
        requires-dist = ["iniconfig"]

        [[package]]
        name = "anyio"
        version = "4.6.0.post2"
        source = { git = "https://github.com/agronholm/anyio?tag=4.6.2#c4844254e6db0cb804c240ba07405db73d810e0b" }
        dependencies = [
            { name = "iniconfig" },
        ]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "anyio", git = "https://github.com/agronholm/anyio?tag=4.6.2" }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    // Re-run with `--offline`. We shouldn't need a network connection to validate an
    // already-correct lockfile with immutable metadata.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + anyio==4.6.2 (from git+https://github.com/agronholm/anyio@c4844254e6db0cb804c240ba07405db73d810e0b)
     + iniconfig==2.0.0
     + project==0.1.0 (from file://[TEMP_DIR]/)
    "###);

    Ok(())
}

#[test]
fn lock_strip_fragment() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl#sha256=b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig", url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl#sha256=b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374" }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0 (from https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl)
     + project==0.1.0 (from file://[TEMP_DIR]/)
    "###);

    Ok(())
}

#[test]
fn lock_request_requires_python() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.8, <=3.10"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    // Request a version that conflicts with `--requires-python`.
    uv_snapshot!(context.filters(), context.lock().arg("--python").arg("3.12"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    error: The requested interpreter resolved to Python 3.12.[X], which is incompatible with the project's Python requirement: `>=3.8, <=3.10`
    "###);

    // Add a `.python-version` file that conflicts.
    let python_version = context.temp_dir.child(".python-version");
    python_version.write_str("3.12")?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    error: The Python request from `.python-version` resolved to Python 3.12.[X], which is incompatible with the project's Python requirement: `>=3.8, <=3.10`. Use `uv python pin` to update the `.python-version` file to a compatible version.
    "###);

    Ok(())
}

#[test]
fn lock_duplicate_sources() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "projeect"
        version = "0.1.0"
        dependencies = ["python-multipart"]

        [tool.uv.sources]
        python-multipart = { url = "https://files.pythonhosted.org/packages/3d/47/444768600d9e0ebc82f8e347775d24aef8f6348cf00e9fa0e81910814e6d/python_multipart-0.0.9-py3-none-any.whl" }
        python-multipart = { url = "https://files.pythonhosted.org/packages/c0/3e/9fbfd74e7f5b54f653f7ca99d44ceb56e718846920162165061c4c22b71a/python_multipart-0.0.8-py3-none-any.whl" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: Failed to parse `pyproject.toml` during settings discovery:
      TOML parse error at line 9, column 9
        |
      9 |         python-multipart = { url = "https://files.pythonhosted.org/packages/c0/3e/9fbfd74e7f5b54f653f7ca99d44ceb56e718846920162165061c4c22b71a/python_multipart-0.0.8-py3-none-any.whl" }
        |         ^
      duplicate key `python-multipart` in table `tool.uv.sources`

    error: Failed to parse: `pyproject.toml`
      Caused by: TOML parse error at line 9, column 9
      |
    9 |         python-multipart = { url = "https://files.pythonhosted.org/packages/c0/3e/9fbfd74e7f5b54f653f7ca99d44ceb56e718846920162165061c4c22b71a/python_multipart-0.0.8-py3-none-any.whl" }
      |         ^
    duplicate key `python-multipart` in table `tool.uv.sources`

    "###);

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        dependencies = ["python-multipart"]

        [tool.uv.sources]
        python-multipart = { url = "https://files.pythonhosted.org/packages/3d/47/444768600d9e0ebc82f8e347775d24aef8f6348cf00e9fa0e81910814e6d/python_multipart-0.0.9-py3-none-any.whl" }
        python_multipart = { url = "https://files.pythonhosted.org/packages/c0/3e/9fbfd74e7f5b54f653f7ca99d44ceb56e718846920162165061c4c22b71a/python_multipart-0.0.8-py3-none-any.whl" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `pyproject.toml`
      Caused by: TOML parse error at line 7, column 9
      |
    7 |         [tool.uv.sources]
      |         ^^^^^^^^^^^^^^^^^
    duplicate sources for package `python-multipart`

    "###);

    Ok(())
}

#[test]
fn lock_invalid_project_table() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("a/pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "a"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["b"]

        [tool.uv.sources]
        b = { path = "../b" }
        "#,
    )?;

    let pyproject_toml = context.temp_dir.child("b/pyproject.toml");
    pyproject_toml.write_str(
        r"
        [project.urls]
        repository = 'https://github.com/octocat/octocat-python'
        ",
    )?;

    uv_snapshot!(context.filters(), context.lock().current_dir(context.temp_dir.join("a")), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
      × Failed to build `b @ file://[TEMP_DIR]/b`
      ├─▶ Failed to extract static metadata from `pyproject.toml`
      ╰─▶ TOML parse error at line 2, column 10
            |
          2 |         [project.urls]
            |          ^^^^^^^
          `pyproject.toml` is using the `[project]` table, but the required `project.name` field is not set
    "###);

    Ok(())
}

#[test]
fn lock_missing_name() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc::indoc! {
        r#"
        [project]
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        "#,
    })?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `pyproject.toml`
      Caused by: TOML parse error at line 1, column 1
      |
    1 | [project]
      | ^^^^^^^^^
    `pyproject.toml` is using the `[project]` table, but the required `project.name` field is not set
    "###);

    Ok(())
}

#[test]
fn lock_missing_version() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc::indoc! {
        r#"
        [project]
        name = "project"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        "#,
    })?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `pyproject.toml`
      Caused by: TOML parse error at line 1, column 1
      |
    1 | [project]
      | ^^^^^^^^^
    `pyproject.toml` is using the `[project]` table, but the required `project.version` field is neither set nor present in the `project.dynamic` list
    "###);

    Ok(())
}

#[test]
fn lock_unsupported_version() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig==2.0.0"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    // Validate schema, invalid version.
    context.temp_dir.child("uv.lock").write_str(
        r#"
        version = 2
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig", specifier = "==2.0.0" }]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().arg("--frozen"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: The lockfile at `uv.lock` uses an unsupported schema version (v2, but only v1 is supported). Downgrade to a compatible uv version, or remove the `uv.lock` prior to running `uv lock` or `uv sync`.
    "###);

    // Invalid schema (`iniconfig` is referenced, but missing), invalid version.
    context.temp_dir.child("uv.lock").write_str(
        r#"
        version = 2
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig", specifier = "==2.0.0" }]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().arg("--frozen"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse `uv.lock`, which uses an unsupported schema version (v2, but only v1 is supported). Downgrade to a compatible uv version, or remove the `uv.lock` prior to running `uv lock` or `uv sync`.
      Caused by: Dependency `iniconfig` has missing `version` field but has more than one matching package
    "###);

    Ok(())
}

/// See: <https://github.com/astral-sh/uv/issues/7618>
#[test]
fn lock_change_requires_python() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio <3 ; python_version == '3.12'",
            "anyio >3, <4 ; python_version > '3.12'",
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        resolution-markers = [
            "python_full_version >= '3.13'",
            "python_full_version < '3.13'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "2.2.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version < '3.13'",
        ]
        dependencies = [
            { name = "idna", marker = "python_full_version < '3.13'" },
            { name = "sniffio", marker = "python_full_version < '3.13'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/d3/e6/901a94731af20e7109415525666cb3753a2bd1edd19616c2730448dffd0d/anyio-2.2.0.tar.gz", hash = "sha256:4a41c5b3a65ed92e469d51b6fba3779301850ea2e352afcf9e36c46f21ee14a9", size = 97217 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/c3/b83a3c02c7d6f66932e9a72621d7f207cbfd2bd72b4c8931567ee386fb55/anyio-2.2.0-py3-none-any.whl", hash = "sha256:aa3da546ed17f097ca876c78024dea380a3b7fa80759abfdda59f12176a3dac8", size = 65320 },
        ]

        [[package]]
        name = "anyio"
        version = "3.7.1"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.13'",
        ]
        dependencies = [
            { name = "idna", marker = "python_full_version >= '3.13'" },
            { name = "sniffio", marker = "python_full_version >= '3.13'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/28/99/2dfd53fd55ce9838e6ff2d4dac20ce58263798bd1a0dbe18b3a9af3fcfce/anyio-3.7.1.tar.gz", hash = "sha256:44a3c9aba0f5defa43261a8b3efb97891f2bd7d804e0e1f56419befa1adfc780", size = 142927 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/19/24/44299477fe7dcc9cb58d0a57d5a7588d6af2ff403fdd2d47a246c91a3246/anyio-3.7.1-py3-none-any.whl", hash = "sha256:91dee416e570e92c64041bd18b900d1d6fa78dff7048769ce5ac5ddad004fbb5", size = 80896 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "anyio", version = "2.2.0", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version < '3.13'" },
            { name = "anyio", version = "3.7.1", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version >= '3.13'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "anyio", marker = "python_full_version == '3.12.*'", specifier = "<3" },
            { name = "anyio", marker = "python_full_version >= '3.13'", specifier = ">3,<4" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    // Lower the `requires-python`, expanding the set of supported versions. Loosen the upper-bound
    // on `anyio` to ensure that we respect the already-locked version.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.10"
        dependencies = [
            "anyio <3 ; python_version == '3.12'",
            "anyio >3 ; python_version > '3.12'",
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.10"
        resolution-markers = [
            "python_full_version >= '3.13'",
            "python_full_version == '3.12.*'",
            "python_full_version < '3.12'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "anyio"
        version = "2.2.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version == '3.12.*'",
        ]
        dependencies = [
            { name = "idna", marker = "python_full_version == '3.12.*'" },
            { name = "sniffio", marker = "python_full_version == '3.12.*'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/d3/e6/901a94731af20e7109415525666cb3753a2bd1edd19616c2730448dffd0d/anyio-2.2.0.tar.gz", hash = "sha256:4a41c5b3a65ed92e469d51b6fba3779301850ea2e352afcf9e36c46f21ee14a9", size = 97217 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/c3/b83a3c02c7d6f66932e9a72621d7f207cbfd2bd72b4c8931567ee386fb55/anyio-2.2.0-py3-none-any.whl", hash = "sha256:aa3da546ed17f097ca876c78024dea380a3b7fa80759abfdda59f12176a3dac8", size = 65320 },
        ]

        [[package]]
        name = "anyio"
        version = "3.7.1"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "python_full_version >= '3.13'",
        ]
        dependencies = [
            { name = "idna", marker = "python_full_version >= '3.13'" },
            { name = "sniffio", marker = "python_full_version >= '3.13'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/28/99/2dfd53fd55ce9838e6ff2d4dac20ce58263798bd1a0dbe18b3a9af3fcfce/anyio-3.7.1.tar.gz", hash = "sha256:44a3c9aba0f5defa43261a8b3efb97891f2bd7d804e0e1f56419befa1adfc780", size = 142927 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/19/24/44299477fe7dcc9cb58d0a57d5a7588d6af2ff403fdd2d47a246c91a3246/anyio-3.7.1-py3-none-any.whl", hash = "sha256:91dee416e570e92c64041bd18b900d1d6fa78dff7048769ce5ac5ddad004fbb5", size = 80896 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "anyio", version = "2.2.0", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version == '3.12.*'" },
            { name = "anyio", version = "3.7.1", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version >= '3.13'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "anyio", marker = "python_full_version == '3.12.*'", specifier = "<3" },
            { name = "anyio", marker = "python_full_version >= '3.13'", specifier = ">3" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    Ok(())
}

/// Pass credentials for a named index via environment variables.
#[test]
fn lock_keyring_credentials() -> Result<()> {
    let keyring_context = TestContext::new("3.12");

    // Install our keyring plugin
    keyring_context
        .pip_install()
        .arg(
            keyring_context
                .workspace_root
                .join("scripts")
                .join("packages")
                .join("keyring_test_plugin"),
        )
        .assert()
        .success();

    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "foo"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv]
        keyring-provider = "subprocess"

        [[tool.uv.index]]
        name = "proxy"
        url = "https://pypi-proxy.fly.dev/basic-auth/simple"
        default = true
        "#,
    )?;

    // Provide credentials via environment variables.
    uv_snapshot!(context.filters(), context.lock()
        .env(EnvVars::index_username("PROXY"), "public")
        .env(EnvVars::KEYRING_TEST_CREDENTIALS, r#"{"pypi-proxy.fly.dev": {"public": "heron"}}"#)
        .env(EnvVars::PATH, venv_bin_path(&keyring_context.venv)), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Request for public@https://pypi-proxy.fly.dev/basic-auth/simple/iniconfig/
    Request for public@pypi-proxy.fly.dev
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock")).unwrap();

    // The lockfile shout omit the credentials.
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "foo"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig" }]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi-proxy.fly.dev/basic-auth/simple" }
        sdist = { url = "https://pypi-proxy.fly.dev/basic-auth/files/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://pypi-proxy.fly.dev/basic-auth/files/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]
        "###
        );
    });

    Ok(())
}

#[test]
fn lock_multiple_sources() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [tool.uv.sources]
        iniconfig = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", marker = "sys_platform != 'win32'" },
            { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", marker = "sys_platform == 'win32'" },
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform != 'win32'",
            "sys_platform == 'win32'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz" }
        resolution-markers = [
            "sys_platform == 'win32'",
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3" }

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl" }
        resolution-markers = [
            "sys_platform != 'win32'",
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "iniconfig", version = "2.0.0", source = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz" }, marker = "sys_platform == 'win32'" },
            { name = "iniconfig", version = "2.0.0", source = { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl" }, marker = "sys_platform != 'win32'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "iniconfig", marker = "sys_platform != 'win32'", url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl" },
            { name = "iniconfig", marker = "sys_platform == 'win32'", url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz" },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    Ok(())
}

#[test]
fn lock_multiple_sources_conflict() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [tool.uv.sources]
        iniconfig = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", marker = "sys_platform == 'win32' and python_version == '3.12'" },
            { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", marker = "sys_platform == 'win32'" },
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `pyproject.toml`
      Caused by: TOML parse error at line 9, column 21
      |
    9 |         iniconfig = [
      |                     ^
    Source markers must be disjoint, but the following markers overlap: `python_full_version == '3.12.*' and sys_platform == 'win32'` and `sys_platform == 'win32'`.

    hint: replace `sys_platform == 'win32'` with `python_full_version != '3.12.*' and sys_platform == 'win32'`.
    "###);

    Ok(())
}

#[test]
fn lock_multiple_sources_no_marker() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [tool.uv.sources]
        iniconfig = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl" },
            { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz" },
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `pyproject.toml`
      Caused by: TOML parse error at line 9, column 21
      |
    9 |         iniconfig = [
      |                     ^
    When multiple sources are provided, each source must include a platform marker (e.g., `marker = "sys_platform == 'linux'"`)
    "###);

    Ok(())
}

#[test]
fn lock_multiple_sources_index_disjoint_markers() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["jinja2>=3"]

        [tool.uv]
        constraint-dependencies = ["markupsafe<3"]

        [tool.uv.sources]
        jinja2 = [
            { index = "torch-cu118", marker = "sys_platform == 'win32'"},
            { index = "torch-cu124", marker = "sys_platform != 'win32'"},
        ]

        [[tool.uv.index]]
        name = "torch-cu118"
        url = "https://download.pytorch.org/whl/cu118"

        [[tool.uv.index]]
        name = "torch-cu124"
        url = "https://download.pytorch.org/whl/cu124"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform == 'win32'",
            "sys_platform != 'win32'",
        ]

        [manifest]
        constraints = [{ name = "markupsafe", specifier = "<3" }]

        [[package]]
        name = "jinja2"
        version = "3.1.3"
        source = { registry = "https://download.pytorch.org/whl/cu118" }
        resolution-markers = [
            "sys_platform == 'win32'",
        ]
        dependencies = [
            { name = "markupsafe", marker = "sys_platform == 'win32'" },
        ]
        wheels = [
            { url = "https://download.pytorch.org/whl/Jinja2-3.1.3-py3-none-any.whl", hash = "sha256:7d6d50dd97d52cbc355597bd845fabfbac3f551e1f99619e39a35ce8c370b5fa" },
        ]

        [[package]]
        name = "jinja2"
        version = "3.1.3"
        source = { registry = "https://download.pytorch.org/whl/cu124" }
        resolution-markers = [
            "sys_platform != 'win32'",
        ]
        dependencies = [
            { name = "markupsafe", marker = "sys_platform != 'win32'" },
        ]
        wheels = [
            { url = "https://download.pytorch.org/whl/Jinja2-3.1.3-py3-none-any.whl", hash = "sha256:7d6d50dd97d52cbc355597bd845fabfbac3f551e1f99619e39a35ce8c370b5fa" },
        ]

        [[package]]
        name = "markupsafe"
        version = "2.1.5"
        source = { registry = "https://download.pytorch.org/whl/cu118" }
        wheels = [
            { url = "https://download.pytorch.org/whl/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:8dec4936e9c3100156f8a2dc89c4b88d5c435175ff03413b443469c7c8c5f4d1" },
            { url = "https://download.pytorch.org/whl/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:3c6b973f22eb18a789b1460b4b91bf04ae3f0c4234a0a6aa6b0a92f6f7b951d4" },
            { url = "https://download.pytorch.org/whl/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ac07bad82163452a6884fe8fa0963fb98c2346ba78d779ec06bd7a6262132aee" },
            { url = "https://download.pytorch.org/whl/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f5dfb42c4604dddc8e4305050aa6deb084540643ed5804d7455b5df8fe16f5e5" },
            { url = "https://download.pytorch.org/whl/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ea3d8a3d18833cf4304cd2fc9cbb1efe188ca9b5efef2bdac7adc20594a0e46b" },
            { url = "https://download.pytorch.org/whl/MarkupSafe-2.1.5-cp312-cp312-win_amd64.whl", hash = "sha256:823b65d8706e32ad2df51ed89496147a42a2a6e01c13cfb6ffb8b1e92bc910bb" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "jinja2", version = "3.1.3", source = { registry = "https://download.pytorch.org/whl/cu118" }, marker = "sys_platform == 'win32'" },
            { name = "jinja2", version = "3.1.3", source = { registry = "https://download.pytorch.org/whl/cu124" }, marker = "sys_platform != 'win32'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "jinja2", marker = "sys_platform != 'win32'", specifier = ">=3", index = "https://download.pytorch.org/whl/cu124" },
            { name = "jinja2", marker = "sys_platform == 'win32'", specifier = ">=3", index = "https://download.pytorch.org/whl/cu118" },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    Ok(())
}

#[test]
fn lock_multiple_sources_index_mixed() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["jinja2>=3"]

        [tool.uv]
        constraint-dependencies = ["markupsafe<3"]

        [tool.uv.sources]
        jinja2 = [
            { index = "torch-cu118", marker = "sys_platform == 'win32'"},
            { url = "https://files.pythonhosted.org/packages/31/80/3a54838c3fb461f6fec263ebf3a3a41771bd05190238de3486aae8540c36/jinja2-3.1.4-py3-none-any.whl", marker = "sys_platform != 'win32'"},
        ]

        [[tool.uv.index]]
        name = "torch-cu118"
        url = "https://download.pytorch.org/whl/cu118"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform == 'win32'",
            "sys_platform != 'win32'",
        ]

        [manifest]
        constraints = [{ name = "markupsafe", specifier = "<3" }]

        [[package]]
        name = "jinja2"
        version = "3.1.3"
        source = { registry = "https://download.pytorch.org/whl/cu118" }
        resolution-markers = [
            "sys_platform == 'win32'",
        ]
        dependencies = [
            { name = "markupsafe", marker = "sys_platform == 'win32'" },
        ]
        wheels = [
            { url = "https://download.pytorch.org/whl/Jinja2-3.1.3-py3-none-any.whl", hash = "sha256:7d6d50dd97d52cbc355597bd845fabfbac3f551e1f99619e39a35ce8c370b5fa" },
        ]

        [[package]]
        name = "jinja2"
        version = "3.1.4"
        source = { url = "https://files.pythonhosted.org/packages/31/80/3a54838c3fb461f6fec263ebf3a3a41771bd05190238de3486aae8540c36/jinja2-3.1.4-py3-none-any.whl" }
        resolution-markers = [
            "sys_platform != 'win32'",
        ]
        dependencies = [
            { name = "markupsafe", marker = "sys_platform != 'win32'" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/31/80/3a54838c3fb461f6fec263ebf3a3a41771bd05190238de3486aae8540c36/jinja2-3.1.4-py3-none-any.whl", hash = "sha256:bc5dd2abb727a5319567b7a813e6a2e7318c39f4f487cfe6c89c6f9c7d25197d" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "babel", marker = "extra == 'i18n'", specifier = ">=2.7" },
            { name = "markupsafe", specifier = ">=2.0" },
        ]

        [[package]]
        name = "markupsafe"
        version = "2.1.5"
        source = { registry = "https://download.pytorch.org/whl/cu118" }
        wheels = [
            { url = "https://download.pytorch.org/whl/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:8dec4936e9c3100156f8a2dc89c4b88d5c435175ff03413b443469c7c8c5f4d1" },
            { url = "https://download.pytorch.org/whl/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:3c6b973f22eb18a789b1460b4b91bf04ae3f0c4234a0a6aa6b0a92f6f7b951d4" },
            { url = "https://download.pytorch.org/whl/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ac07bad82163452a6884fe8fa0963fb98c2346ba78d779ec06bd7a6262132aee" },
            { url = "https://download.pytorch.org/whl/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f5dfb42c4604dddc8e4305050aa6deb084540643ed5804d7455b5df8fe16f5e5" },
            { url = "https://download.pytorch.org/whl/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ea3d8a3d18833cf4304cd2fc9cbb1efe188ca9b5efef2bdac7adc20594a0e46b" },
            { url = "https://download.pytorch.org/whl/MarkupSafe-2.1.5-cp312-cp312-win_amd64.whl", hash = "sha256:823b65d8706e32ad2df51ed89496147a42a2a6e01c13cfb6ffb8b1e92bc910bb" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "jinja2", version = "3.1.3", source = { registry = "https://download.pytorch.org/whl/cu118" }, marker = "sys_platform == 'win32'" },
            { name = "jinja2", version = "3.1.4", source = { url = "https://files.pythonhosted.org/packages/31/80/3a54838c3fb461f6fec263ebf3a3a41771bd05190238de3486aae8540c36/jinja2-3.1.4-py3-none-any.whl" }, marker = "sys_platform != 'win32'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "jinja2", marker = "sys_platform != 'win32'", url = "https://files.pythonhosted.org/packages/31/80/3a54838c3fb461f6fec263ebf3a3a41771bd05190238de3486aae8540c36/jinja2-3.1.4-py3-none-any.whl" },
            { name = "jinja2", marker = "sys_platform == 'win32'", specifier = ">=3", index = "https://download.pytorch.org/whl/cu118" },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    Ok(())
}

#[test]
fn lock_multiple_sources_index_non_total() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["jinja2>=3"]

        [tool.uv.sources]
        jinja2 = [
            { index = "torch-cu118", marker = "sys_platform == 'win32'"},
        ]

        [[tool.uv.index]]
        name = "torch-cu118"
        url = "https://download.pytorch.org/whl/cu118"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    error: Found duplicate package `jinja2==3.1.3 @ registry+https://download.pytorch.org/whl/cu118`
    "###);

    Ok(())
}

#[test]
fn lock_multiple_sources_index_explicit() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["jinja2>=3"]

        [tool.uv.sources]
        jinja2 = [
            { index = "torch-cu118", marker = "sys_platform == 'win32'"},
        ]

        [[tool.uv.index]]
        name = "torch-cu118"
        url = "https://test.pypi.org/simple"
        explicit = true
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform == 'win32'",
            "sys_platform != 'win32'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "jinja2"
        version = "3.1.3"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "sys_platform != 'win32'",
        ]
        dependencies = [
            { name = "markupsafe", marker = "sys_platform != 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b2/5e/3a21abf3cd467d7876045335e681d276ac32492febe6d98ad89562d1a7e1/Jinja2-3.1.3.tar.gz", hash = "sha256:ac8bd6544d4bb2c9792bf3a159e80bba8fda7f07e81bc3aed565432d5925ba90", size = 268261 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/30/6d/6de6be2d02603ab56e72997708809e8a5b0fbfee080735109b40a3564843/Jinja2-3.1.3-py3-none-any.whl", hash = "sha256:7d6d50dd97d52cbc355597bd845fabfbac3f551e1f99619e39a35ce8c370b5fa", size = 133236 },
        ]

        [[package]]
        name = "jinja2"
        version = "3.1.3"
        source = { registry = "https://test.pypi.org/simple" }
        resolution-markers = [
            "sys_platform == 'win32'",
        ]
        dependencies = [
            { name = "markupsafe", marker = "sys_platform == 'win32'" },
        ]
        sdist = { url = "https://test-files.pythonhosted.org/packages/3e/f0/69ae37cced6b277dc0419dbb1c6e4fb259e5e319a1a971061a2776316bec/Jinja2-3.1.3.tar.gz", hash = "sha256:27fb536952e578492fa66d8681d8967d8bdf1eb36368b1f842b53251c9f0bfe1", size = 268254 }
        wheels = [
            { url = "https://test-files.pythonhosted.org/packages/47/dc/9d1c0f1ddbedb1e67f7d00e91819b5a9157056ad83bfa64c12ecef8a4f4e/Jinja2-3.1.3-py3-none-any.whl", hash = "sha256:ddd11470e8a1dc4c30e3146400f0130fed7d85886c5f8082f309355b4b0c1128", size = 133236 },
        ]

        [[package]]
        name = "markupsafe"
        version = "2.1.5"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/87/5b/aae44c6655f3801e81aa3eef09dbbf012431987ba564d7231722f68df02d/MarkupSafe-2.1.5.tar.gz", hash = "sha256:d283d37a890ba4c1ae73ffadf8046435c76e7bc2247bbb63c00bd1a709c6544b", size = 19384 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/53/bd/583bf3e4c8d6a321938c13f49d44024dbe5ed63e0a7ba127e454a66da974/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:8dec4936e9c3100156f8a2dc89c4b88d5c435175ff03413b443469c7c8c5f4d1", size = 18215 },
            { url = "https://files.pythonhosted.org/packages/48/d6/e7cd795fc710292c3af3a06d80868ce4b02bfbbf370b7cee11d282815a2a/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:3c6b973f22eb18a789b1460b4b91bf04ae3f0c4234a0a6aa6b0a92f6f7b951d4", size = 14069 },
            { url = "https://files.pythonhosted.org/packages/51/b5/5d8ec796e2a08fc814a2c7d2584b55f889a55cf17dd1a90f2beb70744e5c/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ac07bad82163452a6884fe8fa0963fb98c2346ba78d779ec06bd7a6262132aee", size = 29452 },
            { url = "https://files.pythonhosted.org/packages/0a/0d/2454f072fae3b5a137c119abf15465d1771319dfe9e4acbb31722a0fff91/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f5dfb42c4604dddc8e4305050aa6deb084540643ed5804d7455b5df8fe16f5e5", size = 28462 },
            { url = "https://files.pythonhosted.org/packages/2d/75/fd6cb2e68780f72d47e6671840ca517bda5ef663d30ada7616b0462ad1e3/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ea3d8a3d18833cf4304cd2fc9cbb1efe188ca9b5efef2bdac7adc20594a0e46b", size = 27869 },
            { url = "https://files.pythonhosted.org/packages/b0/81/147c477391c2750e8fc7705829f7351cf1cd3be64406edcf900dc633feb2/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:d050b3361367a06d752db6ead6e7edeb0009be66bc3bae0ee9d97fb326badc2a", size = 33906 },
            { url = "https://files.pythonhosted.org/packages/8b/ff/9a52b71839d7a256b563e85d11050e307121000dcebc97df120176b3ad93/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:bec0a414d016ac1a18862a519e54b2fd0fc8bbfd6890376898a6c0891dd82e9f", size = 32296 },
            { url = "https://files.pythonhosted.org/packages/88/07/2dc76aa51b481eb96a4c3198894f38b480490e834479611a4053fbf08623/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:58c98fee265677f63a4385256a6d7683ab1832f3ddd1e66fe948d5880c21a169", size = 33038 },
            { url = "https://files.pythonhosted.org/packages/96/0c/620c1fb3661858c0e37eb3cbffd8c6f732a67cd97296f725789679801b31/MarkupSafe-2.1.5-cp312-cp312-win32.whl", hash = "sha256:8590b4ae07a35970728874632fed7bd57b26b0102df2d2b233b6d9d82f6c62ad", size = 16572 },
            { url = "https://files.pythonhosted.org/packages/3f/14/c3554d512d5f9100a95e737502f4a2323a1959f6d0d01e0d0997b35f7b10/MarkupSafe-2.1.5-cp312-cp312-win_amd64.whl", hash = "sha256:823b65d8706e32ad2df51ed89496147a42a2a6e01c13cfb6ffb8b1e92bc910bb", size = 17127 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "jinja2", version = "3.1.3", source = { registry = "https://pypi.org/simple" }, marker = "sys_platform != 'win32'" },
            { name = "jinja2", version = "3.1.3", source = { registry = "https://test.pypi.org/simple" }, marker = "sys_platform == 'win32'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "jinja2", marker = "sys_platform != 'win32'", specifier = ">=3" },
            { name = "jinja2", marker = "sys_platform == 'win32'", specifier = ">=3", index = "https://test.pypi.org/simple" },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    "###);

    Ok(())
}

#[test]
fn lock_multiple_sources_non_total() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [tool.uv.sources]
        iniconfig = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", marker = "sys_platform == 'darwin'" },
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform == 'darwin'",
            "sys_platform != 'darwin'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "sys_platform != 'darwin'",
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "iniconfig", version = "2.0.0", source = { registry = "https://pypi.org/simple" }, marker = "sys_platform != 'darwin'" },
            { name = "iniconfig", version = "2.0.0", source = { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl" }, marker = "sys_platform == 'darwin'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "iniconfig", marker = "sys_platform != 'darwin'" },
            { name = "iniconfig", marker = "sys_platform == 'darwin'", url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl" },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    Ok(())
}

#[test]
fn lock_multiple_sources_respect_marker() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig ; sys_platform == 'win32'"]

        [tool.uv.sources]
        iniconfig = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", marker = "sys_platform == 'darwin'" },
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "iniconfig", marker = "sys_platform == 'win32'" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig", marker = "sys_platform == 'win32'" }]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    Ok(())
}

#[test]
fn lock_multiple_sources_extra() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]

        [project.optional-dependencies]
        cpu = []

        [tool.uv.sources]
        iniconfig = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", marker = "extra == 'cpu'" },
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

        [package.optional-dependencies]
        cpu = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "iniconfig", marker = "extra == 'cpu'", url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl" },
            { name = "iniconfig", marker = "extra != 'cpu'" },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    Ok(())
}

#[test]
fn lock_multiple_sources_index_overlapping_extras() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.optional-dependencies]
        cu118 = ["jinja2==3.1.2"]
        cu124 = ["jinja2==3.1.3"]

        [tool.uv]
        constraint-dependencies = ["markupsafe<3"]

        [tool.uv.sources]
        jinja2 = [
            { index = "torch-cu118", extra = "cu118" },
            { index = "torch-cu124", extra = "cu124" },
        ]

        [[tool.uv.index]]
        name = "torch-cu118"
        url = "https://download.pytorch.org/whl/cu118"
        explicit = true

        [[tool.uv.index]]
        name = "torch-cu124"
        url = "https://download.pytorch.org/whl/cu124"
        explicit = true
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Requirements contain conflicting indexes for package `jinja2` in all marker environments:
    - https://download.pytorch.org/whl/cu118
    - https://download.pytorch.org/whl/cu124
    "###);

    Ok(())
}

/// Sources will be ignored when an `extra` is applied, but references a non-existent extra.
#[test]
fn lock_multiple_index_with_missing_extra() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["jinja2"]

        [tool.uv.sources]
        jinja2 = [
            { index = "torch-cu118", extra = "cu118" },
        ]

        [[tool.uv.index]]
        name = "torch-cu118"
        url = "https://download.pytorch.org/whl/cu118"
        explicit = true
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `project @ file://[TEMP_DIR]/`
      ╰─▶ Source entry for `jinja2` only applies to extra `cu118`, but the `cu118` extra does not exist. When an extra is present on a source (e.g., `extra = "cu118"`), the relevant package must be included in the `project.optional-dependencies` section for that extra (e.g., `project.optional-dependencies = { "cu118" = ["jinja2"] }`).
    "###);

    Ok(())
}

/// Sources will be ignored when an `extra` is applied, but the dependency isn't in an optional
/// group.
#[test]
fn lock_multiple_index_with_absent_extra() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["jinja2>=3"]

        [project.optional-dependencies]
        cu118 = []

        [tool.uv.sources]
        jinja2 = [
            { index = "torch-cu118", extra = "cu118" },
        ]

        [[tool.uv.index]]
        name = "torch-cu118"
        url = "https://download.pytorch.org/whl/cu118"
        explicit = true
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `project @ file://[TEMP_DIR]/`
      ╰─▶ Source entry for `jinja2` only applies to extra `cu118`, but `jinja2` was not found under the `project.optional-dependencies` section for that extra. When an extra is present on a source (e.g., `extra = "cu118"`), the relevant package must be included in the `project.optional-dependencies` section for that extra (e.g., `project.optional-dependencies = { "cu118" = ["jinja2"] }`).
    "###);

    Ok(())
}

/// Sources will be ignored when a `group` is applied, but references a non-existent group.
#[test]
fn lock_multiple_index_with_missing_group() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["jinja2"]

        [tool.uv.sources]
        jinja2 = [
            { index = "torch-cu118", group = "cu118" },
        ]

        [[tool.uv.index]]
        name = "torch-cu118"
        url = "https://download.pytorch.org/whl/cu118"
        explicit = true
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `project @ file://[TEMP_DIR]/`
      ╰─▶ Source entry for `jinja2` only applies to dependency group `cu118`, but the `cu118` group does not exist. When a group is present on a source (e.g., `group = "cu118"`), the relevant package must be included in the `dependency-groups` section for that extra (e.g., `dependency-groups = { "cu118" = ["jinja2"] }`).
    "###);

    Ok(())
}

/// Sources will be ignored when a `group` is applied, but the dependency isn't in a dependency
/// group.
#[test]
fn lock_multiple_index_with_absent_group() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["jinja2>=3"]

        [dependency-groups]
        cu118 = []

        [tool.uv.sources]
        jinja2 = [
            { index = "torch-cu118", group = "cu118" },
        ]

        [[tool.uv.index]]
        name = "torch-cu118"
        url = "https://download.pytorch.org/whl/cu118"
        explicit = true
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `project @ file://[TEMP_DIR]/`
      ╰─▶ Source entry for `jinja2` only applies to dependency group `cu118`, but `jinja2` was not found under the `dependency-groups` section for that group. When a group is present on a source (e.g., `group = "cu118"`), the relevant package must be included in the `dependency-groups` section for that extra (e.g., `dependency-groups = { "cu118" = ["jinja2"] }`).
    "###);

    Ok(())
}

#[test]
fn lock_dry_run() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio <3 ; python_version == '3.12'",
            "anyio >3, <4 ; python_version > '3.12'",
            "matplotlib==3.1.0"
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 12 packages in [TIME]
    "###);

    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "requests==2.25.1",
            "matplotlib==3.5.0"
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().arg("--dry-run"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 19 packages in [TIME]
    Remove anyio v2.2.0, v3.7.1
    Add certifi v2024.2.2
    Add chardet v4.0.0
    Add fonttools v4.50.0
    Update idna v3.6 -> v2.10
    Update matplotlib v3.1.0 -> v3.5.0
    Add packaging v24.0
    Add pillow v10.2.0
    Add requests v2.25.1
    Add setuptools v69.2.0
    Add setuptools-scm v8.0.4
    Remove sniffio v1.3.1
    Add typing-extensions v4.10.0
    Add urllib3 v1.26.18
    "###);

    Ok(())
}

#[test]
fn lock_dry_run_noop() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio <3 ; python_version == '3.12'",
            "anyio >3, <4 ; python_version > '3.12'",
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().arg("--dry-run"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Add anyio v2.2.0, v3.7.1
    Add idna v3.6
    Add project v0.1.0
    Add sniffio v1.3.1
    "###);

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###);

    uv_snapshot!(context.filters(), context.lock().arg("--dry-run"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    No lockfile changes detected
    "###);

    uv_snapshot!(context.filters(), context.lock().arg("--dry-run").arg("-U"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    "###);

    Ok(())
}

#[test]
fn lock_group_include() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [dependency-groups]
        foo = ["anyio", {include-group = "bar"}]
        bar = ["trio"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 11 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
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
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "attrs"
        version = "23.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/e3/fc/f800d51204003fa8ae392c4e8278f256206e7a919b708eef054f5f4b650d/attrs-23.2.0.tar.gz", hash = "sha256:935dc3b529c262f6cf76e50877d35a4bd3c1de194fd41f47a2b7ae8f19971f30", size = 780820 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e0/44/827b2a91a5816512fcaf3cc4ebc465ccd5d598c45cefa6703fcf4a79018f/attrs-23.2.0-py3-none-any.whl", hash = "sha256:99b87a485a5820b23b879f04c2305b44b951b502fd64be915879d77a7e8fc6f1", size = 60752 },
        ]

        [[package]]
        name = "cffi"
        version = "1.16.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "pycparser" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/68/ce/95b0bae7968c65473e1298efb042e10cafc7bafc14d9e4f154008241c91d/cffi-1.16.0.tar.gz", hash = "sha256:bcb3ef43e58665bbda2fb198698fcae6776483e0c4a631aa5647806c25e02cc0", size = 512873 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c9/6e/751437067affe7ac0944b1ad4856ec11650da77f0dd8f305fae1117ef7bb/cffi-1.16.0-cp312-cp312-win32.whl", hash = "sha256:b2ca4e77f9f47c55c194982e10f058db063937845bb2b7a86c84a6cfe0aefa8b", size = 173564 },
            { url = "https://files.pythonhosted.org/packages/e9/63/e285470a4880a4f36edabe4810057bd4b562c6ddcc165eacf9c3c7210b40/cffi-1.16.0-cp312-cp312-win_amd64.whl", hash = "sha256:68678abf380b42ce21a5f2abde8efee05c114c2fdb2e9eef2efdb0257fba1235", size = 181956 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "outcome"
        version = "1.3.0.post0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "attrs" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/98/df/77698abfac98571e65ffeb0c1fba8ffd692ab8458d617a0eed7d9a8d38f2/outcome-1.3.0.post0.tar.gz", hash = "sha256:9dcf02e65f2971b80047b377468e72a268e15c0af3cf1238e6ff14f7f91143b8", size = 21060 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/8b/5ab7257531a5d830fc8000c476e63c935488d74609b50f9384a643ec0a62/outcome-1.3.0.post0-py2.py3-none-any.whl", hash = "sha256:e771c5ce06d1415e356078d3bdd68523f284b4ce5419828922b6871e65eda82b", size = 10692 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "typing-extensions" },
        ]

        [package.dev-dependencies]
        bar = [
            { name = "trio" },
        ]
        foo = [
            { name = "anyio" },
            { name = "trio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "typing-extensions" }]

        [package.metadata.requires-dev]
        bar = [{ name = "trio" }]
        foo = [
            { name = "anyio" },
            { name = "trio" },
        ]

        [[package]]
        name = "pycparser"
        version = "2.21"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/5e/0b/95d387f5f4433cb0f53ff7ad859bd2c6051051cebbb564f139a999ab46de/pycparser-2.21.tar.gz", hash = "sha256:e644fdec12f7872f86c58ff790da456218b10f863970249516d60a5eaca77206", size = 170877 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/62/d5/5f610ebe421e85889f2e55e33b7f9a6795bd982198517d912eb1c76e1a53/pycparser-2.21-py2.py3-none-any.whl", hash = "sha256:8ee45429555515e1f6b185e78100aea234072576aa43ab53aefcae078162fca9", size = 118697 },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]

        [[package]]
        name = "sortedcontainers"
        version = "2.4.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/e8/c4/ba2f8066cceb6f23394729afe52f3bf7adec04bf9ed2c820b39e19299111/sortedcontainers-2.4.0.tar.gz", hash = "sha256:25caa5a06cc30b6b83d11423433f65d1f9d76c4c6a0c90e3379eaa43b9bfdb88", size = 30594 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/32/46/9cb0e58b2deb7f82b84065f37f3bffeb12413f947f9388e4cac22c4621ce/sortedcontainers-2.4.0-py2.py3-none-any.whl", hash = "sha256:a163dcaede0f1c021485e957a39245190e74249897e2ae4b2aa38595db237ee0", size = 29575 },
        ]

        [[package]]
        name = "trio"
        version = "0.25.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "attrs" },
            { name = "cffi", marker = "implementation_name != 'pypy' and os_name == 'nt'" },
            { name = "idna" },
            { name = "outcome" },
            { name = "sniffio" },
            { name = "sortedcontainers" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b4/51/4f5ae37ec58768b9c30e5bc5b89431a7baf3fa9d0dda98983af6ef55eb47/trio-0.25.0.tar.gz", hash = "sha256:9b41f5993ad2c0e5f62d0acca320ec657fdb6b2a2c22b8c7aed6caf154475c4e", size = 551863 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/17/c9/f86f89f14d52f9f2f652ce24cb2f60141a51d087db1563f3fba94ba07346/trio-0.25.0-py3-none-any.whl", hash = "sha256:e6458efe29cc543e557a91e614e2b51710eba2961669329ce9c862d50c6e8e81", size = 467161 },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 },
        ]
        "###
        );
    });

    Ok(())
}

#[test]
fn lock_group_include_cycle() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [dependency-groups]
        foo = ["anyio", {include-group = "bar"}]
        bar = [{include-group = "foobar"}]
        foobar = [{include-group = "foo"}]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `project @ file://[TEMP_DIR]/`
      ╰─▶ Detected a cycle in `dependency-groups`: `bar` -> `foobar` -> `foo` -> `bar`
    "###);

    Ok(())
}

#[test]
fn lock_group_include_dev() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        dev-dependencies = ["anyio"]

        [dependency-groups]
        foo = ["typing-extensions", {include-group = "dev"}]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `project @ file://[TEMP_DIR]/`
      ╰─▶ Group `foo` includes the `dev` group (`include = "dev"`), but only `tool.uv.dev-dependencies` was found. To reference the `dev` group via an `include`, remove the `tool.uv.dev-dependencies` section and add any development dependencies to the `dev` entry in the `[dependency-groups]` table instead.
    "###);

    Ok(())
}

#[test]
fn lock_group_include_missing() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [dependency-groups]
        foo = ["anyio", {include-group = "bar"}]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `project @ file://[TEMP_DIR]/`
      ╰─▶ Failed to find group `bar` included by `foo`
    "###);

    Ok(())
}

#[test]
fn lock_group_invalid_entry_package() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [dependency-groups]
        foo = ["invalid!"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `project @ file://[TEMP_DIR]/`
      ├─▶ Failed to parse entry in group `foo`: `invalid!`
      ╰─▶ no such comparison operator "!", must be one of ~= == != <= >= < > ===
          invalid!
                 ^
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--group").arg("foo"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `project @ file://[TEMP_DIR]/`
      ├─▶ Failed to parse entry in group `foo`: `invalid!`
      ╰─▶ no such comparison operator "!", must be one of ~= == != <= >= < > ===
          invalid!
                 ^
    "###);

    Ok(())
}

#[test]
fn lock_group_invalid_entry_group_name() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [dependency-groups]
        foo = [{include-group = "invalid!"}]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `pyproject.toml`
      Caused by: TOML parse error at line 9, column 16
      |
    9 |         foo = [{include-group = "invalid!"}]
      |                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    Not a valid package or extra name: "invalid!". Names must start and end with a letter or digit and may only contain -, _, ., and alphanumeric characters.

    "###);

    Ok(())
}

#[test]
fn lock_group_invalid_duplicate_group_name() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [dependency-groups]
        foo-bar = []
        foo_bar = []
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `pyproject.toml`
      Caused by: TOML parse error at line 8, column 9
      |
    8 |         [dependency-groups]
      |         ^^^^^^^^^^^^^^^^^^^
    duplicate dependency group: `foo-bar`
    "###);

    Ok(())
}

#[test]
fn lock_group_invalid_entry_table() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [dependency-groups]
        foo = [{bar = "unknown"}]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    Ok(())
}

#[test]
fn lock_group_invalid_entry_type() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [dependency-groups]
        foo = [{include-group = true}]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `pyproject.toml`
      Caused by: TOML parse error at line 9, column 33
      |
    9 |         foo = [{include-group = true}]
      |                                 ^^^^
    invalid type: boolean `true`, expected a string

    "###);

    Ok(())
}

#[test]
fn lock_group_empty_entry_table() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [dependency-groups]
        foo = [{}]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `pyproject.toml`
      Caused by: TOML parse error at line 9, column 16
      |
    9 |         foo = [{}]
      |                ^^
    missing field `include-group`

    "###);

    Ok(())
}

#[test]
fn lock_group_workspace() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["child"]

        [dependency-groups]
        types = ["sniffio>1"]
        async = ["anyio>3"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.uv.workspace]
        members = ["child"]

        [tool.uv.sources]
        child = { workspace = true }
        "#,
    )?;

    // Add a workspace member.
    let child = context.temp_dir.child("child");
    child.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig>1"]

        [dependency-groups]
        types = ["typing-extensions>4"]
        testing = ["pytest>8"]

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 11 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        members = [
            "child",
            "project",
        ]

        [[package]]
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "child"
        version = "0.1.0"
        source = { editable = "child" }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.dev-dependencies]
        testing = [
            { name = "pytest" },
        ]
        types = [
            { name = "typing-extensions" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig", specifier = ">1" }]

        [package.metadata.requires-dev]
        testing = [{ name = "pytest", specifier = ">8" }]
        types = [{ name = "typing-extensions", specifier = ">4" }]

        [[package]]
        name = "colorama"
        version = "0.4.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d8/53/6f443c9a4a8358a93a6792e2acffb9d9d5cb0a5cfd8802644b7b1c9a02e4/colorama-0.4.6.tar.gz", hash = "sha256:08695f5cb7ed6e0531a20572697297273c47b8cae5a63ffc6d6ed5c201be6e44", size = 27697 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d1/d6/3965ed04c63042e047cb6a3e6ed1a63a35087b6a609aa3a15ed8ac56c221/colorama-0.4.6-py2.py3-none-any.whl", hash = "sha256:4f1d9991f5acc0ca119f9d443620b77f9d6b33703e51011c16baf57afb285fc6", size = 25335 },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488 },
        ]

        [[package]]
        name = "pluggy"
        version = "1.4.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/54/c6/43f9d44d92aed815e781ca25ba8c174257e27253a94630d21be8725a2b59/pluggy-1.4.0.tar.gz", hash = "sha256:8c85c2876142a764e5b7548e7d9a0e0ddb46f5185161049a79b7e974454223be", size = 65812 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a5/5b/0cc789b59e8cc1bf288b38111d002d8c5917123194d45b29dcdac64723cc/pluggy-1.4.0-py3-none-any.whl", hash = "sha256:7db9f7b503d67d1c5b95f59773ebb58a8c1c288129a88665838012cfb07b8981", size = 20120 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "child" },
        ]

        [package.dev-dependencies]
        async = [
            { name = "anyio" },
        ]
        types = [
            { name = "sniffio" },
        ]

        [package.metadata]
        requires-dist = [{ name = "child", editable = "child" }]

        [package.metadata.requires-dev]
        async = [{ name = "anyio", specifier = ">3" }]
        types = [{ name = "sniffio", specifier = ">1" }]

        [[package]]
        name = "pytest"
        version = "8.1.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "colorama", marker = "sys_platform == 'win32'" },
            { name = "iniconfig" },
            { name = "packaging" },
            { name = "pluggy" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/30/b7/7d44bbc04c531dcc753056920e0988032e5871ac674b5a84cb979de6e7af/pytest-8.1.1.tar.gz", hash = "sha256:ac978141a75948948817d360297b7aae0fcb9d6ff6bc9ec6d514b85d5a65c044", size = 1409703 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/4d/7e/c79cecfdb6aa85c6c2e3cf63afc56d0f165f24f5c66c03c695c4d9b84756/pytest-8.1.1-py3-none-any.whl", hash = "sha256:2a8386cfc11fa9d2c50ee7b2a57e7d898ef90470a7a34c4b949ff59662bb78b7", size = 337359 },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 },
        ]
        "###
        );
    });

    Ok(())
}

#[test]
fn lock_transitive_git() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "a"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["c"]

        [tool.uv.sources]
        c = { git = "https://github.com/astral-sh/workspace-virtual-root-test", subdirectory = "packages/c", rev = "fac39c8d4c5d0ef32744e2bb309bbe34a759fd46" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "a"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "c" },
        ]

        [package.metadata]
        requires-dist = [{ name = "c", git = "https://github.com/astral-sh/workspace-virtual-root-test?subdirectory=packages%2Fc&rev=fac39c8d4c5d0ef32744e2bb309bbe34a759fd46" }]

        [[package]]
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
        ]

        [[package]]
        name = "c"
        version = "1.0.0"
        source = { git = "https://github.com/astral-sh/workspace-virtual-root-test?subdirectory=packages%2Fc&rev=fac39c8d4c5d0ef32744e2bb309bbe34a759fd46#fac39c8d4c5d0ef32744e2bb309bbe34a759fd46" }
        dependencies = [
            { name = "d" },
        ]

        [[package]]
        name = "d"
        version = "1.0.0"
        source = { git = "https://github.com/astral-sh/workspace-virtual-root-test?subdirectory=packages%2Fd&rev=fac39c8d4c5d0ef32744e2bb309bbe34a759fd46#fac39c8d4c5d0ef32744e2bb309bbe34a759fd46" }
        dependencies = [
            { name = "anyio" },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "###);

    // Re-run with `--offline`. We shouldn't need a network connection to validate an
    // already-correct lockfile with immutable metadata.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + anyio==4.3.0
     + c==1.0.0 (from git+https://github.com/astral-sh/workspace-virtual-root-test@fac39c8d4c5d0ef32744e2bb309bbe34a759fd46#subdirectory=packages/c)
     + d==1.0.0 (from git+https://github.com/astral-sh/workspace-virtual-root-test@fac39c8d4c5d0ef32744e2bb309bbe34a759fd46#subdirectory=packages/d)
     + idna==3.6
     + sniffio==1.3.1
    "###);

    Ok(())
}

/// Lock a package that's excluded from the parent workspace, but depends on that parent.
#[test]
fn lock_dynamic_version() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        requires-python = ">=3.12"
        dynamic = ["version"]

        [build-system]
        requires = ["setuptools"]
        build-backend = "setuptools.build_meta"

        [tool.uv]
        cache-keys = [{ file = "pyproject.toml" }, { file = "src/project/__init__.py" }]

        [tool.setuptools.dynamic]
        version = { attr = "project.__version__" }

        [tool.setuptools]
        package-dir = { "" = "src" }

        [tool.setuptools.packages.find]
        where = ["src"]
        "#,
    )?;

    context
        .temp_dir
        .child("src")
        .child("project")
        .child("__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        "###
        );
    });

    // Bump the version.
    context
        .temp_dir
        .child("src")
        .child("project")
        .child("__init__.py")
        .write_str("__version__ = '0.1.1'")?;

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided. To update the lockfile, run `uv lock`.
    "###);

    // Re-lock.
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Updated project v0.1.0 -> v0.1.1
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock")).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.1"
        source = { editable = "." }
        "###
        );
    });

    Ok(())
}

#[test]
fn lock_derivation_chain_prod() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["wsgiref==0.1.2"]
        "#,
    )?;

    let filters = context
        .filters()
        .into_iter()
        .chain([
            (r"exit code: 1", "exit status: 1"),
            (r"/.*/src", "/[TMP]/src"),
        ])
        .collect::<Vec<_>>();

    uv_snapshot!(filters, context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `wsgiref==0.1.2`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `setuptools.build_meta:__legacy__.build_wheel` failed (exit status: 1)

          [stderr]
          Traceback (most recent call last):
            File "<string>", line 14, in <module>
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 325, in get_requires_for_build_wheel
              return self._get_build_requires(config_settings, requirements=['wheel'])
                     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 295, in _get_build_requires
              self.run_setup()
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 487, in run_setup
              super().run_setup(setup_script=setup_script)
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 311, in run_setup
              exec(code, locals())
            File "<string>", line 5, in <module>
            File "[CACHE_DIR]/[TMP]/src/ez_setup/__init__.py", line 170
              print "Setuptools version",version,"or greater has been installed."
              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
          SyntaxError: Missing parentheses in call to 'print'. Did you mean print(...)?

          hint: This usually indicates a problem with the package or the build environment.
      help: `wsgiref` (v0.1.2) was included because `project` (v0.1.0) depends on `wsgiref==0.1.2`
    "###);

    Ok(())
}

#[test]
fn lock_derivation_chain_extra() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
        optional-dependencies = { wsgi = ["wsgiref>=0.1"] }
        "#,
    )?;

    let filters = context
        .filters()
        .into_iter()
        .chain([
            (r"exit code: 1", "exit status: 1"),
            (r"/.*/src", "/[TMP]/src"),
        ])
        .collect::<Vec<_>>();

    uv_snapshot!(filters, context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `wsgiref==0.1.2`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `setuptools.build_meta:__legacy__.build_wheel` failed (exit status: 1)

          [stderr]
          Traceback (most recent call last):
            File "<string>", line 14, in <module>
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 325, in get_requires_for_build_wheel
              return self._get_build_requires(config_settings, requirements=['wheel'])
                     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 295, in _get_build_requires
              self.run_setup()
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 487, in run_setup
              super().run_setup(setup_script=setup_script)
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 311, in run_setup
              exec(code, locals())
            File "<string>", line 5, in <module>
            File "[CACHE_DIR]/[TMP]/src/ez_setup/__init__.py", line 170
              print "Setuptools version",version,"or greater has been installed."
              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
          SyntaxError: Missing parentheses in call to 'print'. Did you mean print(...)?

          hint: This usually indicates a problem with the package or the build environment.
      help: `wsgiref` (v0.1.2) was included because `project[wsgi]` (v0.1.0) depends on `wsgiref>=0.1`
    "###);

    Ok(())
}

#[test]
fn lock_derivation_chain_group() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [dependency-groups]
        wsgi = ["wsgiref"]
        "#,
    )?;

    let filters = context
        .filters()
        .into_iter()
        .chain([
            (r"exit code: 1", "exit status: 1"),
            (r"/.*/src", "/[TMP]/src"),
        ])
        .collect::<Vec<_>>();

    uv_snapshot!(filters, context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `wsgiref==0.1.2`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `setuptools.build_meta:__legacy__.build_wheel` failed (exit status: 1)

          [stderr]
          Traceback (most recent call last):
            File "<string>", line 14, in <module>
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 325, in get_requires_for_build_wheel
              return self._get_build_requires(config_settings, requirements=['wheel'])
                     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 295, in _get_build_requires
              self.run_setup()
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 487, in run_setup
              super().run_setup(setup_script=setup_script)
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 311, in run_setup
              exec(code, locals())
            File "<string>", line 5, in <module>
            File "[CACHE_DIR]/[TMP]/src/ez_setup/__init__.py", line 170
              print "Setuptools version",version,"or greater has been installed."
              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
          SyntaxError: Missing parentheses in call to 'print'. Did you mean print(...)?

          hint: This usually indicates a problem with the package or the build environment.
      help: `wsgiref` (v0.1.2) was included because `project:wsgi` (v0.1.0) depends on `wsgiref`
    "###);

    Ok(())
}

#[test]
fn lock_derivation_chain_extended() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["child"]

        [tool.uv.sources]
        child = { path = "child" }
        "#,
    )?;

    let child = context.temp_dir.child("child");
    child.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["wsgiref>=0.1, <0.2"]
        "#,
    )?;

    let filters = context
        .filters()
        .into_iter()
        .chain([
            (r"exit code: 1", "exit status: 1"),
            (r"/.*/src", "/[TMP]/src"),
        ])
        .collect::<Vec<_>>();

    uv_snapshot!(filters, context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `wsgiref==0.1.2`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `setuptools.build_meta:__legacy__.build_wheel` failed (exit status: 1)

          [stderr]
          Traceback (most recent call last):
            File "<string>", line 14, in <module>
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 325, in get_requires_for_build_wheel
              return self._get_build_requires(config_settings, requirements=['wheel'])
                     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 295, in _get_build_requires
              self.run_setup()
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 487, in run_setup
              super().run_setup(setup_script=setup_script)
            File "[CACHE_DIR]/builds-v0/[TMP]/build_meta.py", line 311, in run_setup
              exec(code, locals())
            File "<string>", line 5, in <module>
            File "[CACHE_DIR]/[TMP]/src/ez_setup/__init__.py", line 170
              print "Setuptools version",version,"or greater has been installed."
              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
          SyntaxError: Missing parentheses in call to 'print'. Did you mean print(...)?

          hint: This usually indicates a problem with the package or the build environment.
      help: `wsgiref` (v0.1.2) was included because `project` (v0.1.0) depends on `child` (v0.1.0) which depends on `wsgiref>=0.1, <0.2`
    "###);

    Ok(())
}

/// The project itself is marked as an editable dependency, but under the wrong name. The project
/// itself isn't a package.
#[test]
fn mismatched_name_self_editable() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["foo"]

        [tool.uv.sources]
        foo = { path = ".", editable = true }
        "#,
    )?;

    // Running `uv sync` should generate a lockfile.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
      × Failed to build `foo @ file://[TEMP_DIR]/`
      ╰─▶ Package metadata name `project` does not match given name `foo`
      help: `foo` was included because `project` (v0.1.0) depends on `foo`
    "###);

    Ok(())
}

#[test]
fn lock_relative_project() -> Result<()> {
    let context = TestContext::new("3.12");

    context
        .temp_dir
        .child("project")
        .child("pyproject.toml")
        .write_str(
            r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]
        "#,
        )?;

    let peer = context.temp_dir.child("peer");
    fs_err::create_dir_all(&peer)?;

    uv_snapshot!(context.filters(), context.lock().arg("--project").arg("../project").current_dir(&peer), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read(context.temp_dir.child("project").child("uv.lock"));

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "typing-extensions" },
        ]

        [package.metadata]
        requires-dist = [{ name = "typing-extensions" }]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--project").arg("../project").current_dir(&peer), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 2 packages in [TIME]
    "###);

    // Create a virtual environment in the project directory.
    context
        .command()
        .arg("venv")
        .current_dir(context.temp_dir.child("project"))
        .assert()
        .success();

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--project").arg("../project").current_dir(&peer), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.10.0
    "###);

    Ok(())
}

#[test]
fn lock_recursive_extra() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.optional-dependencies]
        foo = ["iniconfig"]
        bar = ["project[foo]"]
        baz = ["project[bar]"]
        bop = ["project[bar] ; sys_platform == 'darwin'"]
        qux = ["project[bop] ; python_version == '3.12'"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

        [package.optional-dependencies]
        bar = [
            { name = "iniconfig" },
        ]
        baz = [
            { name = "iniconfig" },
        ]
        bop = [
            { name = "iniconfig", marker = "sys_platform == 'darwin'" },
        ]
        foo = [
            { name = "iniconfig" },
        ]
        qux = [
            { name = "iniconfig", marker = "python_full_version < '3.13' and sys_platform == 'darwin'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "iniconfig", marker = "extra == 'foo'" },
            { name = "project", extras = ["bar"], marker = "sys_platform == 'darwin' and extra == 'bop'" },
            { name = "project", extras = ["bar"], marker = "extra == 'baz'" },
            { name = "project", extras = ["bop"], marker = "python_full_version == '3.12.*' and extra == 'qux'" },
            { name = "project", extras = ["foo"], marker = "extra == 'bar'" },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    Ok(())
}

/// Don't show an unpinned lower bound warning if the package was provided both by name and by URL,
/// or if the package was part of a constraints files.
///
/// See <https://github.com/astral-sh/uv/issues/8155>, the original example was:
/// ```
/// uv pip install dist/pymatgen-2024.10.3.tar.gz pymatgen[ci,optional] --resolution=lowest
/// ```
#[test]
fn no_lowest_warning_with_name_and_url() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio[trio]",
            "anyio @ https://files.pythonhosted.org/packages/e6/e3/c4c8d473d6780ef1853d630d581f70d655b4f8d7553c6997958c283039a2/anyio-4.4.0.tar.gz"
        ]

        [tool.uv]
        constraint-dependencies = [
            "sortedcontainers==2.4.0",
            "outcome==1.3.0.post0",
            "pycparser==2.20",
        ]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    "###);

    Ok(())
}

#[test]
fn lock_no_build_static_metadata() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "dummy"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().arg("--no-build"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "dummy"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [{ name = "iniconfig" }]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--no-build").arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Re-run with `--offline`. We shouldn't need a network connection to validate an
    // already-correct lockfile with immutable metadata.
    uv_snapshot!(context.filters(), context.lock().arg("--no-build").arg("--locked").arg("--offline").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--no-build").arg("--frozen"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Distribution `dummy==0.1.0 @ virtual+.` can't be installed because it is marked as `--no-build` but has no binary distribution
    "###);

    Ok(())
}

#[test]
fn lock_no_build_dynamic_metadata() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "dummy"
        version = "0.1.0"
        requires-python = ">=3.12"
        dynamic = ["dependencies"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().arg("--no-build"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `dummy @ file://[TEMP_DIR]/`
      ╰─▶ Building source distributions for dummy is disabled
    "###);

    Ok(())
}

#[test]
fn lock_self_compatible() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions", "project"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "typing-extensions" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "project" },
            { name = "typing-extensions" },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Re-run with `--offline`. We shouldn't need a network connection to validate an
    // already-correct lockfile with immutable metadata.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.10.0
    "###);

    Ok(())
}

#[test]
fn lock_self_exact() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions", "project==0.1.0"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "typing-extensions" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "project", specifier = "==0.1.0" },
            { name = "typing-extensions" },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Re-run with `--offline`. We shouldn't need a network connection to validate an
    // already-correct lockfile with immutable metadata.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.10.0
    "###);

    Ok(())
}

#[test]
fn lock_self_incompatible() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions", "project==0.2.0"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because your project depends on itself at an incompatible version (project==0.2.0), we can conclude that your project's requirements are unsatisfiable.

          hint: The project `project` depends on itself at an incompatible version. This is likely a mistake. If you intended to depend on a third-party package named `project`, consider renaming the project `project` to avoid creating a conflict.
    "###);

    Ok(())
}

#[test]
fn lock_self_extra_to_extra_compatible() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [project.optional-dependencies]
        foo = ["project[foo]"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "typing-extensions" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "project", extras = ["foo"], marker = "extra == 'foo'" },
            { name = "typing-extensions" },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Re-run with `--offline`. We shouldn't need a network connection to validate an
    // already-correct lockfile with immutable metadata.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.10.0
    "###);

    Ok(())
}

#[test]
fn lock_self_extra_to_same_extra_incompatible() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [project.optional-dependencies]
        foo = ["project[foo]==0.2.0"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because project[foo] depends on itself at an incompatible version (project==0.2.0) and your project requires project[foo], we can conclude that your project's requirements are unsatisfiable.

          hint: The project `project` depends on itself at an incompatible version. This is likely a mistake. If you intended to depend on a third-party package named `project`, consider renaming the project `project` to avoid creating a conflict.
    "###);

    Ok(())
}

#[test]
fn lock_self_extra_to_other_extra_incompatible() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [project.optional-dependencies]
        foo = ["project[bar]==0.2.0"]
        bar = ["iniconfig"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because project[foo] depends on itself at an incompatible version (project==0.2.0) and your project requires project[foo], we can conclude that your project's requirements are unsatisfiable.

          hint: The project `project` depends on itself at an incompatible version. This is likely a mistake. If you intended to depend on a third-party package named `project`, consider renaming the project `project` to avoid creating a conflict.
    "###);

    Ok(())
}

#[test]
fn lock_self_extra_compatible() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [project.optional-dependencies]
        foo = ["project"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "typing-extensions" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "project", marker = "extra == 'foo'" },
            { name = "typing-extensions" },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Re-run with `--offline`. We shouldn't need a network connection to validate an
    // already-correct lockfile with immutable metadata.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.10.0
    "###);

    Ok(())
}

#[test]
fn lock_self_extra_incompatible() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions"]

        [project.optional-dependencies]
        foo = ["project==0.2.0"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because project[foo] depends on itself at an incompatible version (project==0.2.0) and your project requires project[foo], we can conclude that your project's requirements are unsatisfiable.

          hint: The project `project` depends on itself at an incompatible version. This is likely a mistake. If you intended to depend on a third-party package named `project`, consider renaming the project `project` to avoid creating a conflict.
    "###);

    Ok(())
}

#[test]
fn lock_self_marker_compatible() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions", "project ; sys_platform == 'win32'"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "typing-extensions" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "project", marker = "sys_platform == 'win32'" },
            { name = "typing-extensions" },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 },
        ]
        "###
        );
    });

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Re-run with `--offline`. We shouldn't need a network connection to validate an
    // already-correct lockfile with immutable metadata.
    uv_snapshot!(context.filters(), context.lock().arg("--locked").arg("--offline").arg("--no-cache"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + typing-extensions==4.10.0
    "###);

    Ok(())
}

#[test]
fn lock_self_marker_incompatible() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["typing-extensions", "project>0.1 ; sys_platform == 'win32'"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because only project{sys_platform == 'win32'}<=0.1 is available and your project depends on itself at an incompatible version (project{sys_platform == 'win32'}>0.1), we can conclude that your project's requirements are unsatisfiable.

          hint: The project `project` depends on itself at an incompatible version. This is likely a mistake. If you intended to depend on a third-party package named `project`, consider renaming the project `project` to avoid creating a conflict.
    "###);

    Ok(())
}

/// When resolving `PyQt5-Qt5`, we choose the latest version, even though it doesn't support
/// Windows. This may change in the future.
#[test]
fn lock_split_on_windows() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["pyqt5-qt5"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "pyqt5-qt5" },
        ]

        [package.metadata]
        requires-dist = [{ name = "pyqt5-qt5" }]

        [[package]]
        name = "pyqt5-qt5"
        version = "5.15.13"
        source = { registry = "https://pypi.org/simple" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/40/dc/96d9d0ba0d13256343b53efffe8729f278e62409ab4c937bb22e70ab98ac/PyQt5_Qt5-5.15.13-py3-none-macosx_10_13_x86_64.whl", hash = "sha256:92575a9e96a27c4ed67c56c7048ded7461a1655d5d21f0e05064664e6e9fcbdf", size = 38771962 },
            { url = "https://files.pythonhosted.org/packages/c9/8b/4441c208c8ca29b50fab6467ebfa32b6401d16c5c915a031a48dc85dfa7a/PyQt5_Qt5-5.15.13-py3-none-macosx_11_0_arm64.whl", hash = "sha256:141859f2ffe04cc6c5db970e2b6ad9f98897805d886a14c52614e3799daab6d6", size = 36663754 },
        ]
        "###
        );
    });

    Ok(())
}

#[test]
fn lock_missing_git_prefix() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["workspace-in-root-test"]

        [tool.uv.sources]
        workspace-in-root-test = { url = "https://github.com/astral-sh/workspace-in-root-test" }

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `project @ file://[TEMP_DIR]/`
      ├─▶ Failed to parse entry: `workspace-in-root-test`
      ╰─▶ `workspace-in-root-test` is associated with a URL source, but references a Git repository. Consider using a Git source instead (e.g., `workspace-in-root-test = { git = "https://github.com/astral-sh/workspace-in-root-test" }`)
    "###);

    Ok(())
}

#[test]
fn lock_pytorch_cpu() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12.0"
        dependencies = []

        [project.optional-dependencies]
        cpu = [
          "torch>=2.5.1",
          "torchvision>=0.20.1",
        ]
        cu124 = [
          "torch>=2.5.1",
          "torchvision>=0.20.1",
        ]

        [tool.uv]
        conflicts = [
          [
            { extra = "cpu" },
            { extra = "cu124" },
          ],
        ]

        [tool.uv.sources]
        torch = [
          { index = "pytorch-cpu", extra = "cpu" },
          { index = "pytorch-cu124", extra = "cu124" },
        ]
        torchvision = [
          { index = "pytorch-cpu", extra = "cpu" },
          { index = "pytorch-cu124", extra = "cu124" },
        ]

        [[tool.uv.index]]
        name = "pytorch-cpu"
        url = "https://download.pytorch.org/whl/cpu"
        explicit = true

        [[tool.uv.index]]
        name = "pytorch-cu124"
        url = "https://download.pytorch.org/whl/cu124"
        explicit = true

        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove(EnvVars::UV_EXCLUDE_NEWER), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 31 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12.[X]"
        resolution-markers = [
            "sys_platform != 'darwin'",
            "sys_platform == 'darwin'",
        ]
        conflicts = [[
            { package = "project", extra = "cpu" },
            { package = "project", extra = "cu124" },
        ]]

        [[package]]
        name = "filelock"
        version = "3.16.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/9d/db/3ef5bb276dae18d6ec2124224403d1d67bccdbefc17af4cc8f553e341ab1/filelock-3.16.1.tar.gz", hash = "sha256:c249fbfcd5db47e5e2d6d62198e565475ee65e4831e2561c8e313fa7eb961435", size = 18037 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b9/f8/feced7779d755758a52d1f6635d990b8d98dc0a29fa568bbe0625f18fdf3/filelock-3.16.1-py3-none-any.whl", hash = "sha256:2082e5703d51fbf98ea75855d9d5527e33d8ff23099bec374a134febee6946b0", size = 16163 },
        ]

        [[package]]
        name = "fsspec"
        version = "2024.12.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/11/de70dee31455c546fbc88301971ec03c328f3d1138cfba14263f651e9551/fsspec-2024.12.0.tar.gz", hash = "sha256:670700c977ed2fb51e0d9f9253177ed20cbde4a3e5c0283cc5385b5870c8533f", size = 291600 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/de/86/5486b0188d08aa643e127774a99bac51ffa6cf343e3deb0583956dca5b22/fsspec-2024.12.0-py3-none-any.whl", hash = "sha256:b520aed47ad9804237ff878b504267a3b0b441e97508bd6d2d8774e3db85cee2", size = 183862 },
        ]

        [[package]]
        name = "jinja2"
        version = "3.1.4"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "markupsafe" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/ed/55/39036716d19cab0747a5020fc7e907f362fbf48c984b14e62127f7e68e5d/jinja2-3.1.4.tar.gz", hash = "sha256:4a3aee7acbbe7303aede8e9648d13b8bf88a429282aa6122a993f0ac800cb369", size = 240245 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/31/80/3a54838c3fb461f6fec263ebf3a3a41771bd05190238de3486aae8540c36/jinja2-3.1.4-py3-none-any.whl", hash = "sha256:bc5dd2abb727a5319567b7a813e6a2e7318c39f4f487cfe6c89c6f9c7d25197d", size = 133271 },
        ]

        [[package]]
        name = "markupsafe"
        version = "3.0.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/b2/97/5d42485e71dfc078108a86d6de8fa46db44a1a9295e89c5d6d4a06e23a62/markupsafe-3.0.2.tar.gz", hash = "sha256:ee55d3edf80167e48ea11a923c7386f4669df67d7994554387f84e7d8b0a2bf0", size = 20537 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/22/09/d1f21434c97fc42f09d290cbb6350d44eb12f09cc62c9476effdb33a18aa/MarkupSafe-3.0.2-cp312-cp312-macosx_10_13_universal2.whl", hash = "sha256:9778bd8ab0a994ebf6f84c2b949e65736d5575320a17ae8984a77fab08db94cf", size = 14274 },
            { url = "https://files.pythonhosted.org/packages/6b/b0/18f76bba336fa5aecf79d45dcd6c806c280ec44538b3c13671d49099fdd0/MarkupSafe-3.0.2-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:846ade7b71e3536c4e56b386c2a47adf5741d2d8b94ec9dc3e92e5e1ee1e2225", size = 12348 },
            { url = "https://files.pythonhosted.org/packages/e0/25/dd5c0f6ac1311e9b40f4af06c78efde0f3b5cbf02502f8ef9501294c425b/MarkupSafe-3.0.2-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:1c99d261bd2d5f6b59325c92c73df481e05e57f19837bdca8413b9eac4bd8028", size = 24149 },
            { url = "https://files.pythonhosted.org/packages/f3/f0/89e7aadfb3749d0f52234a0c8c7867877876e0a20b60e2188e9850794c17/MarkupSafe-3.0.2-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:e17c96c14e19278594aa4841ec148115f9c7615a47382ecb6b82bd8fea3ab0c8", size = 23118 },
            { url = "https://files.pythonhosted.org/packages/d5/da/f2eeb64c723f5e3777bc081da884b414671982008c47dcc1873d81f625b6/MarkupSafe-3.0.2-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:88416bd1e65dcea10bc7569faacb2c20ce071dd1f87539ca2ab364bf6231393c", size = 22993 },
            { url = "https://files.pythonhosted.org/packages/da/0e/1f32af846df486dce7c227fe0f2398dc7e2e51d4a370508281f3c1c5cddc/MarkupSafe-3.0.2-cp312-cp312-musllinux_1_2_aarch64.whl", hash = "sha256:2181e67807fc2fa785d0592dc2d6206c019b9502410671cc905d132a92866557", size = 24178 },
            { url = "https://files.pythonhosted.org/packages/c4/f6/bb3ca0532de8086cbff5f06d137064c8410d10779c4c127e0e47d17c0b71/MarkupSafe-3.0.2-cp312-cp312-musllinux_1_2_i686.whl", hash = "sha256:52305740fe773d09cffb16f8ed0427942901f00adedac82ec8b67752f58a1b22", size = 23319 },
            { url = "https://files.pythonhosted.org/packages/a2/82/8be4c96ffee03c5b4a034e60a31294daf481e12c7c43ab8e34a1453ee48b/MarkupSafe-3.0.2-cp312-cp312-musllinux_1_2_x86_64.whl", hash = "sha256:ad10d3ded218f1039f11a75f8091880239651b52e9bb592ca27de44eed242a48", size = 23352 },
            { url = "https://files.pythonhosted.org/packages/51/ae/97827349d3fcffee7e184bdf7f41cd6b88d9919c80f0263ba7acd1bbcb18/MarkupSafe-3.0.2-cp312-cp312-win32.whl", hash = "sha256:0f4ca02bea9a23221c0182836703cbf8930c5e9454bacce27e767509fa286a30", size = 15097 },
            { url = "https://files.pythonhosted.org/packages/c1/80/a61f99dc3a936413c3ee4e1eecac96c0da5ed07ad56fd975f1a9da5bc630/MarkupSafe-3.0.2-cp312-cp312-win_amd64.whl", hash = "sha256:8e06879fc22a25ca47312fbe7c8264eb0b662f6db27cb2d3bbbc74b1df4b9b87", size = 15601 },
            { url = "https://files.pythonhosted.org/packages/83/0e/67eb10a7ecc77a0c2bbe2b0235765b98d164d81600746914bebada795e97/MarkupSafe-3.0.2-cp313-cp313-macosx_10_13_universal2.whl", hash = "sha256:ba9527cdd4c926ed0760bc301f6728ef34d841f405abf9d4f959c478421e4efd", size = 14274 },
            { url = "https://files.pythonhosted.org/packages/2b/6d/9409f3684d3335375d04e5f05744dfe7e9f120062c9857df4ab490a1031a/MarkupSafe-3.0.2-cp313-cp313-macosx_11_0_arm64.whl", hash = "sha256:f8b3d067f2e40fe93e1ccdd6b2e1d16c43140e76f02fb1319a05cf2b79d99430", size = 12352 },
            { url = "https://files.pythonhosted.org/packages/d2/f5/6eadfcd3885ea85fe2a7c128315cc1bb7241e1987443d78c8fe712d03091/MarkupSafe-3.0.2-cp313-cp313-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:569511d3b58c8791ab4c2e1285575265991e6d8f8700c7be0e88f86cb0672094", size = 24122 },
            { url = "https://files.pythonhosted.org/packages/0c/91/96cf928db8236f1bfab6ce15ad070dfdd02ed88261c2afafd4b43575e9e9/MarkupSafe-3.0.2-cp313-cp313-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:15ab75ef81add55874e7ab7055e9c397312385bd9ced94920f2802310c930396", size = 23085 },
            { url = "https://files.pythonhosted.org/packages/c2/cf/c9d56af24d56ea04daae7ac0940232d31d5a8354f2b457c6d856b2057d69/MarkupSafe-3.0.2-cp313-cp313-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:f3818cb119498c0678015754eba762e0d61e5b52d34c8b13d770f0719f7b1d79", size = 22978 },
            { url = "https://files.pythonhosted.org/packages/2a/9f/8619835cd6a711d6272d62abb78c033bda638fdc54c4e7f4272cf1c0962b/MarkupSafe-3.0.2-cp313-cp313-musllinux_1_2_aarch64.whl", hash = "sha256:cdb82a876c47801bb54a690c5ae105a46b392ac6099881cdfb9f6e95e4014c6a", size = 24208 },
            { url = "https://files.pythonhosted.org/packages/f9/bf/176950a1792b2cd2102b8ffeb5133e1ed984547b75db47c25a67d3359f77/MarkupSafe-3.0.2-cp313-cp313-musllinux_1_2_i686.whl", hash = "sha256:cabc348d87e913db6ab4aa100f01b08f481097838bdddf7c7a84b7575b7309ca", size = 23357 },
            { url = "https://files.pythonhosted.org/packages/ce/4f/9a02c1d335caabe5c4efb90e1b6e8ee944aa245c1aaaab8e8a618987d816/MarkupSafe-3.0.2-cp313-cp313-musllinux_1_2_x86_64.whl", hash = "sha256:444dcda765c8a838eaae23112db52f1efaf750daddb2d9ca300bcae1039adc5c", size = 23344 },
            { url = "https://files.pythonhosted.org/packages/ee/55/c271b57db36f748f0e04a759ace9f8f759ccf22b4960c270c78a394f58be/MarkupSafe-3.0.2-cp313-cp313-win32.whl", hash = "sha256:bcf3e58998965654fdaff38e58584d8937aa3096ab5354d493c77d1fdd66d7a1", size = 15101 },
            { url = "https://files.pythonhosted.org/packages/29/88/07df22d2dd4df40aba9f3e402e6dc1b8ee86297dddbad4872bd5e7b0094f/MarkupSafe-3.0.2-cp313-cp313-win_amd64.whl", hash = "sha256:e6a2a455bd412959b57a172ce6328d2dd1f01cb2135efda2e4576e8a23fa3b0f", size = 15603 },
            { url = "https://files.pythonhosted.org/packages/62/6a/8b89d24db2d32d433dffcd6a8779159da109842434f1dd2f6e71f32f738c/MarkupSafe-3.0.2-cp313-cp313t-macosx_10_13_universal2.whl", hash = "sha256:b5a6b3ada725cea8a5e634536b1b01c30bcdcd7f9c6fff4151548d5bf6b3a36c", size = 14510 },
            { url = "https://files.pythonhosted.org/packages/7a/06/a10f955f70a2e5a9bf78d11a161029d278eeacbd35ef806c3fd17b13060d/MarkupSafe-3.0.2-cp313-cp313t-macosx_11_0_arm64.whl", hash = "sha256:a904af0a6162c73e3edcb969eeeb53a63ceeb5d8cf642fade7d39e7963a22ddb", size = 12486 },
            { url = "https://files.pythonhosted.org/packages/34/cf/65d4a571869a1a9078198ca28f39fba5fbb910f952f9dbc5220afff9f5e6/MarkupSafe-3.0.2-cp313-cp313t-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:4aa4e5faecf353ed117801a068ebab7b7e09ffb6e1d5e412dc852e0da018126c", size = 25480 },
            { url = "https://files.pythonhosted.org/packages/0c/e3/90e9651924c430b885468b56b3d597cabf6d72be4b24a0acd1fa0e12af67/MarkupSafe-3.0.2-cp313-cp313t-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:c0ef13eaeee5b615fb07c9a7dadb38eac06a0608b41570d8ade51c56539e509d", size = 23914 },
            { url = "https://files.pythonhosted.org/packages/66/8c/6c7cf61f95d63bb866db39085150df1f2a5bd3335298f14a66b48e92659c/MarkupSafe-3.0.2-cp313-cp313t-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:d16a81a06776313e817c951135cf7340a3e91e8c1ff2fac444cfd75fffa04afe", size = 23796 },
            { url = "https://files.pythonhosted.org/packages/bb/35/cbe9238ec3f47ac9a7c8b3df7a808e7cb50fe149dc7039f5f454b3fba218/MarkupSafe-3.0.2-cp313-cp313t-musllinux_1_2_aarch64.whl", hash = "sha256:6381026f158fdb7c72a168278597a5e3a5222e83ea18f543112b2662a9b699c5", size = 25473 },
            { url = "https://files.pythonhosted.org/packages/e6/32/7621a4382488aa283cc05e8984a9c219abad3bca087be9ec77e89939ded9/MarkupSafe-3.0.2-cp313-cp313t-musllinux_1_2_i686.whl", hash = "sha256:3d79d162e7be8f996986c064d1c7c817f6df3a77fe3d6859f6f9e7be4b8c213a", size = 24114 },
            { url = "https://files.pythonhosted.org/packages/0d/80/0985960e4b89922cb5a0bac0ed39c5b96cbc1a536a99f30e8c220a996ed9/MarkupSafe-3.0.2-cp313-cp313t-musllinux_1_2_x86_64.whl", hash = "sha256:131a3c7689c85f5ad20f9f6fb1b866f402c445b220c19fe4308c0b147ccd2ad9", size = 24098 },
            { url = "https://files.pythonhosted.org/packages/82/78/fedb03c7d5380df2427038ec8d973587e90561b2d90cd472ce9254cf348b/MarkupSafe-3.0.2-cp313-cp313t-win32.whl", hash = "sha256:ba8062ed2cf21c07a9e295d5b8a2a5ce678b913b45fdf68c32d95d6c1291e0b6", size = 15208 },
            { url = "https://files.pythonhosted.org/packages/4f/65/6079a46068dfceaeabb5dcad6d674f5f5c61a6fa5673746f42a9f4c233b3/MarkupSafe-3.0.2-cp313-cp313t-win_amd64.whl", hash = "sha256:e444a31f8db13eb18ada366ab3cf45fd4b31e4db1236a4448f68778c1d1a5a2f", size = 15739 },
        ]

        [[package]]
        name = "mpmath"
        version = "1.3.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/e0/47/dd32fa426cc72114383ac549964eecb20ecfd886d1e5ccf5340b55b02f57/mpmath-1.3.0.tar.gz", hash = "sha256:7a28eb2a9774d00c7bc92411c19a89209d5da7c4c9a9e227be8330a23a25b91f", size = 508106 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/43/e3/7d92a15f894aa0c9c4b49b8ee9ac9850d6e63b03c9c32c0367a13ae62209/mpmath-1.3.0-py3-none-any.whl", hash = "sha256:a0b2b9fe80bbcd81a6647ff13108738cfb482d481d826cc0e02f5b35e5c88d2c", size = 536198 },
        ]

        [[package]]
        name = "networkx"
        version = "3.4.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/fd/1d/06475e1cd5264c0b870ea2cc6fdb3e37177c1e565c43f56ff17a10e3937f/networkx-3.4.2.tar.gz", hash = "sha256:307c3669428c5362aab27c8a1260aa8f47c4e91d3891f48be0141738d8d053e1", size = 2151368 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b9/54/dd730b32ea14ea797530a4479b2ed46a6fb250f682a9cfb997e968bf0261/networkx-3.4.2-py3-none-any.whl", hash = "sha256:df5d4365b724cf81b8c6a7312509d0c22386097011ad1abe274afd5e9d3bbc5f", size = 1723263 },
        ]

        [[package]]
        name = "numpy"
        version = "2.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/47/1b/1d565e0f6e156e1522ab564176b8b29d71e13d8caf003a08768df3d5cec5/numpy-2.2.0.tar.gz", hash = "sha256:140dd80ff8981a583a60980be1a655068f8adebf7a45a06a6858c873fcdcd4a0", size = 20225497 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7f/bc/a20dc4e1d051149052762e7647455311865d11c603170c476d1e910a353e/numpy-2.2.0-cp312-cp312-macosx_10_13_x86_64.whl", hash = "sha256:cff210198bb4cae3f3c100444c5eaa573a823f05c253e7188e1362a5555235b3", size = 20909153 },
            { url = "https://files.pythonhosted.org/packages/60/3d/ac4fb63f36db94f4c7db05b45e3ecb3f88f778ca71850664460c78cfde41/numpy-2.2.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:58b92a5828bd4d9aa0952492b7de803135038de47343b2aa3cc23f3b71a3dc4e", size = 14095021 },
            { url = "https://files.pythonhosted.org/packages/41/6d/a654d519d24e4fcc7a83d4a51209cda086f26cf30722b3d8ffc1aa9b775e/numpy-2.2.0-cp312-cp312-macosx_14_0_arm64.whl", hash = "sha256:ebe5e59545401fbb1b24da76f006ab19734ae71e703cdb4a8b347e84a0cece67", size = 5125491 },
            { url = "https://files.pythonhosted.org/packages/e6/22/fab7e1510a62e5092f4e6507a279020052b89f11d9cfe52af7f52c243b04/numpy-2.2.0-cp312-cp312-macosx_14_0_x86_64.whl", hash = "sha256:e2b8cd48a9942ed3f85b95ca4105c45758438c7ed28fff1e4ce3e57c3b589d8e", size = 6658534 },
            { url = "https://files.pythonhosted.org/packages/fc/29/a3d938ddc5a534cd53df7ab79d20a68db8c67578de1df0ae0118230f5f54/numpy-2.2.0-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:57fcc997ffc0bef234b8875a54d4058afa92b0b0c4223fc1f62f24b3b5e86038", size = 14046306 },
            { url = "https://files.pythonhosted.org/packages/90/24/d0bbb56abdd8934f30384632e3c2ca1ebfeb5d17e150c6e366ba291de36b/numpy-2.2.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:85ad7d11b309bd132d74397fcf2920933c9d1dc865487128f5c03d580f2c3d03", size = 16095819 },
            { url = "https://files.pythonhosted.org/packages/99/9c/58a673faa9e8a0e77248e782f7a17410cf7259b326265646fd50ed49c4e1/numpy-2.2.0-cp312-cp312-musllinux_1_2_aarch64.whl", hash = "sha256:cb24cca1968b21355cc6f3da1a20cd1cebd8a023e3c5b09b432444617949085a", size = 15243215 },
            { url = "https://files.pythonhosted.org/packages/9c/61/f311693f78cbf635cfb69ce9e1e857ff83937a27d93c96ac5932fd33e330/numpy-2.2.0-cp312-cp312-musllinux_1_2_x86_64.whl", hash = "sha256:0798b138c291d792f8ea40fe3768610f3c7dd2574389e37c3f26573757c8f7ef", size = 17860175 },
            { url = "https://files.pythonhosted.org/packages/11/3e/491c34262cb1fc9dd13a00beb80d755ee0517b17db20e54cac7aa524533e/numpy-2.2.0-cp312-cp312-win32.whl", hash = "sha256:afe8fb968743d40435c3827632fd36c5fbde633b0423da7692e426529b1759b1", size = 6273281 },
            { url = "https://files.pythonhosted.org/packages/89/ea/00537f599eb230771157bc509f6ea5b2dddf05d4b09f9d2f1d7096a18781/numpy-2.2.0-cp312-cp312-win_amd64.whl", hash = "sha256:3a4199f519e57d517ebd48cb76b36c82da0360781c6a0353e64c0cac30ecaad3", size = 12613227 },
            { url = "https://files.pythonhosted.org/packages/bd/4c/0d1eef206545c994289e7a9de21b642880a11e0ed47a2b0c407c688c4f69/numpy-2.2.0-cp313-cp313-macosx_10_13_x86_64.whl", hash = "sha256:f8c8b141ef9699ae777c6278b52c706b653bf15d135d302754f6b2e90eb30367", size = 20895707 },
            { url = "https://files.pythonhosted.org/packages/16/cb/88f6c1e6df83002c421d5f854ccf134aa088aa997af786a5dac3f32ec99b/numpy-2.2.0-cp313-cp313-macosx_11_0_arm64.whl", hash = "sha256:0f0986e917aca18f7a567b812ef7ca9391288e2acb7a4308aa9d265bd724bdae", size = 14110592 },
            { url = "https://files.pythonhosted.org/packages/b4/54/817e6894168a43f33dca74199ba0dd0f1acd99aa6323ed6d323d63d640a2/numpy-2.2.0-cp313-cp313-macosx_14_0_arm64.whl", hash = "sha256:1c92113619f7b272838b8d6702a7f8ebe5edea0df48166c47929611d0b4dea69", size = 5110858 },
            { url = "https://files.pythonhosted.org/packages/c7/99/00d8a1a8eb70425bba7880257ed73fed08d3e8d05da4202fb6b9a81d5ee4/numpy-2.2.0-cp313-cp313-macosx_14_0_x86_64.whl", hash = "sha256:5a145e956b374e72ad1dff82779177d4a3c62bc8248f41b80cb5122e68f22d13", size = 6645143 },
            { url = "https://files.pythonhosted.org/packages/34/86/5b9c2b7c56e7a9d9297a0a4be0b8433f498eba52a8f5892d9132b0f64627/numpy-2.2.0-cp313-cp313-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:18142b497d70a34b01642b9feabb70156311b326fdddd875a9981f34a369b671", size = 14042812 },
            { url = "https://files.pythonhosted.org/packages/df/54/13535f74391dbe5f479ceed96f1403267be302c840040700d4fd66688089/numpy-2.2.0-cp313-cp313-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:a7d41d1612c1a82b64697e894b75db6758d4f21c3ec069d841e60ebe54b5b571", size = 16093419 },
            { url = "https://files.pythonhosted.org/packages/dd/37/dfb2056842ac61315f225aa56f455da369f5223e4c5a38b91d20da1b628b/numpy-2.2.0-cp313-cp313-musllinux_1_2_aarch64.whl", hash = "sha256:a98f6f20465e7618c83252c02041517bd2f7ea29be5378f09667a8f654a5918d", size = 15238969 },
            { url = "https://files.pythonhosted.org/packages/5a/3d/d20d24ee313992f0b7e7b9d9eef642d9b545d39d5b91c4a2cc8c98776328/numpy-2.2.0-cp313-cp313-musllinux_1_2_x86_64.whl", hash = "sha256:e09d40edfdb4e260cb1567d8ae770ccf3b8b7e9f0d9b5c2a9992696b30ce2742", size = 17855705 },
            { url = "https://files.pythonhosted.org/packages/5b/40/944c9ee264f875a2db6f79380944fd2b5bb9d712bb4a134d11f45ad5b693/numpy-2.2.0-cp313-cp313-win32.whl", hash = "sha256:3905a5fffcc23e597ee4d9fb3fcd209bd658c352657548db7316e810ca80458e", size = 6270078 },
            { url = "https://files.pythonhosted.org/packages/30/04/e1ee6f8b22034302d4c5c24e15782bdedf76d90b90f3874ed0b48525def0/numpy-2.2.0-cp313-cp313-win_amd64.whl", hash = "sha256:a184288538e6ad699cbe6b24859206e38ce5fba28f3bcfa51c90d0502c1582b2", size = 12605791 },
            { url = "https://files.pythonhosted.org/packages/ef/fb/51d458625cd6134d60ac15180ae50995d7d21b0f2f92a6286ae7b0792d19/numpy-2.2.0-cp313-cp313t-macosx_10_13_x86_64.whl", hash = "sha256:7832f9e8eb00be32f15fdfb9a981d6955ea9adc8574c521d48710171b6c55e95", size = 20920160 },
            { url = "https://files.pythonhosted.org/packages/b4/34/162ae0c5d2536ea4be98c813b5161c980f0443cd5765fde16ddfe3450140/numpy-2.2.0-cp313-cp313t-macosx_11_0_arm64.whl", hash = "sha256:f0dd071b95bbca244f4cb7f70b77d2ff3aaaba7fa16dc41f58d14854a6204e6c", size = 14119064 },
            { url = "https://files.pythonhosted.org/packages/17/6c/4195dd0e1c41c55f466d516e17e9e28510f32af76d23061ea3da67438e3c/numpy-2.2.0-cp313-cp313t-macosx_14_0_arm64.whl", hash = "sha256:b0b227dcff8cdc3efbce66d4e50891f04d0a387cce282fe1e66199146a6a8fca", size = 5152778 },
            { url = "https://files.pythonhosted.org/packages/2f/47/ea804ae525832c8d05ed85b560dfd242d34e4bb0962bc269ccaa720fb934/numpy-2.2.0-cp313-cp313t-macosx_14_0_x86_64.whl", hash = "sha256:6ab153263a7c5ccaf6dfe7e53447b74f77789f28ecb278c3b5d49db7ece10d6d", size = 6667605 },
            { url = "https://files.pythonhosted.org/packages/76/99/34d20e50b3d894bb16b5374bfbee399ab8ff3a33bf1e1f0b8acfe7bbd70d/numpy-2.2.0-cp313-cp313t-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:e500aba968a48e9019e42c0c199b7ec0696a97fa69037bea163b55398e390529", size = 14013275 },
            { url = "https://files.pythonhosted.org/packages/69/8f/a1df7bd02d434ab82539517d1b98028985700cfc4300bc5496fb140ca648/numpy-2.2.0-cp313-cp313t-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:440cfb3db4c5029775803794f8638fbdbf71ec702caf32735f53b008e1eaece3", size = 16074900 },
            { url = "https://files.pythonhosted.org/packages/04/94/b419e7a76bf21a00fcb03c613583f10e389fdc8dfe420412ff5710c8ad3d/numpy-2.2.0-cp313-cp313t-musllinux_1_2_aarch64.whl", hash = "sha256:a55dc7a7f0b6198b07ec0cd445fbb98b05234e8b00c5ac4874a63372ba98d4ab", size = 15219122 },
            { url = "https://files.pythonhosted.org/packages/65/d9/dddf398b2b6c5d750892a207a469c2854a8db0f033edaf72103af8cf05aa/numpy-2.2.0-cp313-cp313t-musllinux_1_2_x86_64.whl", hash = "sha256:4bddbaa30d78c86329b26bd6aaaea06b1e47444da99eddac7bf1e2fab717bd72", size = 17851668 },
            { url = "https://files.pythonhosted.org/packages/d4/dc/09a4e5819a9782a213c0eb4eecacdc1cd75ad8dac99279b04cfccb7eeb0a/numpy-2.2.0-cp313-cp313t-win32.whl", hash = "sha256:30bf971c12e4365153afb31fc73f441d4da157153f3400b82db32d04de1e4066", size = 6325288 },
            { url = "https://files.pythonhosted.org/packages/ce/e1/e0d06ec34036c92b43aef206efe99a5f5f04e12c776eab82a36e00c40afc/numpy-2.2.0-cp313-cp313t-win_amd64.whl", hash = "sha256:d35717333b39d1b6bb8433fa758a55f1081543de527171543a2b710551d40881", size = 12692303 },
        ]

        [[package]]
        name = "nvidia-cublas-cu12"
        version = "12.4.5.8"
        source = { registry = "https://pypi.org/simple" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7f/7f/7fbae15a3982dc9595e49ce0f19332423b260045d0a6afe93cdbe2f1f624/nvidia_cublas_cu12-12.4.5.8-py3-none-manylinux2014_aarch64.whl", hash = "sha256:0f8aa1706812e00b9f19dfe0cdb3999b092ccb8ca168c0db5b8ea712456fd9b3", size = 363333771 },
            { url = "https://files.pythonhosted.org/packages/ae/71/1c91302526c45ab494c23f61c7a84aa568b8c1f9d196efa5993957faf906/nvidia_cublas_cu12-12.4.5.8-py3-none-manylinux2014_x86_64.whl", hash = "sha256:2fc8da60df463fdefa81e323eef2e36489e1c94335b5358bcb38360adf75ac9b", size = 363438805 },
            { url = "https://files.pythonhosted.org/packages/e2/2a/4f27ca96232e8b5269074a72e03b4e0d43aa68c9b965058b1684d07c6ff8/nvidia_cublas_cu12-12.4.5.8-py3-none-win_amd64.whl", hash = "sha256:5a796786da89203a0657eda402bcdcec6180254a8ac22d72213abc42069522dc", size = 396895858 },
        ]

        [[package]]
        name = "nvidia-cuda-cupti-cu12"
        version = "12.4.127"
        source = { registry = "https://pypi.org/simple" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/93/b5/9fb3d00386d3361b03874246190dfec7b206fd74e6e287b26a8fcb359d95/nvidia_cuda_cupti_cu12-12.4.127-py3-none-manylinux2014_aarch64.whl", hash = "sha256:79279b35cf6f91da114182a5ce1864997fd52294a87a16179ce275773799458a", size = 12354556 },
            { url = "https://files.pythonhosted.org/packages/67/42/f4f60238e8194a3106d06a058d494b18e006c10bb2b915655bd9f6ea4cb1/nvidia_cuda_cupti_cu12-12.4.127-py3-none-manylinux2014_x86_64.whl", hash = "sha256:9dec60f5ac126f7bb551c055072b69d85392b13311fcc1bcda2202d172df30fb", size = 13813957 },
            { url = "https://files.pythonhosted.org/packages/f3/79/8cf313ec17c58ccebc965568e5bcb265cdab0a1df99c4e674bb7a3b99bfe/nvidia_cuda_cupti_cu12-12.4.127-py3-none-win_amd64.whl", hash = "sha256:5688d203301ab051449a2b1cb6690fbe90d2b372f411521c86018b950f3d7922", size = 9938035 },
        ]

        [[package]]
        name = "nvidia-cuda-nvrtc-cu12"
        version = "12.4.127"
        source = { registry = "https://pypi.org/simple" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/77/aa/083b01c427e963ad0b314040565ea396f914349914c298556484f799e61b/nvidia_cuda_nvrtc_cu12-12.4.127-py3-none-manylinux2014_aarch64.whl", hash = "sha256:0eedf14185e04b76aa05b1fea04133e59f465b6f960c0cbf4e37c3cb6b0ea198", size = 24133372 },
            { url = "https://files.pythonhosted.org/packages/2c/14/91ae57cd4db3f9ef7aa99f4019cfa8d54cb4caa7e00975df6467e9725a9f/nvidia_cuda_nvrtc_cu12-12.4.127-py3-none-manylinux2014_x86_64.whl", hash = "sha256:a178759ebb095827bd30ef56598ec182b85547f1508941a3d560eb7ea1fbf338", size = 24640306 },
            { url = "https://files.pythonhosted.org/packages/7c/30/8c844bfb770f045bcd8b2c83455c5afb45983e1a8abf0c4e5297b481b6a5/nvidia_cuda_nvrtc_cu12-12.4.127-py3-none-win_amd64.whl", hash = "sha256:a961b2f1d5f17b14867c619ceb99ef6fcec12e46612711bcec78eb05068a60ec", size = 19751955 },
        ]

        [[package]]
        name = "nvidia-cuda-runtime-cu12"
        version = "12.4.127"
        source = { registry = "https://pypi.org/simple" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a1/aa/b656d755f474e2084971e9a297def515938d56b466ab39624012070cb773/nvidia_cuda_runtime_cu12-12.4.127-py3-none-manylinux2014_aarch64.whl", hash = "sha256:961fe0e2e716a2a1d967aab7caee97512f71767f852f67432d572e36cb3a11f3", size = 894177 },
            { url = "https://files.pythonhosted.org/packages/ea/27/1795d86fe88ef397885f2e580ac37628ed058a92ed2c39dc8eac3adf0619/nvidia_cuda_runtime_cu12-12.4.127-py3-none-manylinux2014_x86_64.whl", hash = "sha256:64403288fa2136ee8e467cdc9c9427e0434110899d07c779f25b5c068934faa5", size = 883737 },
            { url = "https://files.pythonhosted.org/packages/a8/8b/450e93fab75d85a69b50ea2d5fdd4ff44541e0138db16f9cd90123ef4de4/nvidia_cuda_runtime_cu12-12.4.127-py3-none-win_amd64.whl", hash = "sha256:09c2e35f48359752dfa822c09918211844a3d93c100a715d79b59591130c5e1e", size = 878808 },
        ]

        [[package]]
        name = "nvidia-cudnn-cu12"
        version = "9.1.0.70"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "nvidia-cublas-cu12" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/9f/fd/713452cd72343f682b1c7b9321e23829f00b842ceaedcda96e742ea0b0b3/nvidia_cudnn_cu12-9.1.0.70-py3-none-manylinux2014_x86_64.whl", hash = "sha256:165764f44ef8c61fcdfdfdbe769d687e06374059fbb388b6c89ecb0e28793a6f", size = 664752741 },
            { url = "https://files.pythonhosted.org/packages/3f/d0/f90ee6956a628f9f04bf467932c0a25e5a7e706a684b896593c06c82f460/nvidia_cudnn_cu12-9.1.0.70-py3-none-win_amd64.whl", hash = "sha256:6278562929433d68365a07a4a1546c237ba2849852c0d4b2262a486e805b977a", size = 679925892 },
        ]

        [[package]]
        name = "nvidia-cufft-cu12"
        version = "11.2.1.3"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "nvidia-nvjitlink-cu12" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7a/8a/0e728f749baca3fbeffad762738276e5df60851958be7783af121a7221e7/nvidia_cufft_cu12-11.2.1.3-py3-none-manylinux2014_aarch64.whl", hash = "sha256:5dad8008fc7f92f5ddfa2101430917ce2ffacd86824914c82e28990ad7f00399", size = 211422548 },
            { url = "https://files.pythonhosted.org/packages/27/94/3266821f65b92b3138631e9c8e7fe1fb513804ac934485a8d05776e1dd43/nvidia_cufft_cu12-11.2.1.3-py3-none-manylinux2014_x86_64.whl", hash = "sha256:f083fc24912aa410be21fa16d157fed2055dab1cc4b6934a0e03cba69eb242b9", size = 211459117 },
            { url = "https://files.pythonhosted.org/packages/f6/ee/3f3f8e9874f0be5bbba8fb4b62b3de050156d159f8b6edc42d6f1074113b/nvidia_cufft_cu12-11.2.1.3-py3-none-win_amd64.whl", hash = "sha256:d802f4954291101186078ccbe22fc285a902136f974d369540fd4a5333d1440b", size = 210576476 },
        ]

        [[package]]
        name = "nvidia-curand-cu12"
        version = "10.3.5.147"
        source = { registry = "https://pypi.org/simple" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/80/9c/a79180e4d70995fdf030c6946991d0171555c6edf95c265c6b2bf7011112/nvidia_curand_cu12-10.3.5.147-py3-none-manylinux2014_aarch64.whl", hash = "sha256:1f173f09e3e3c76ab084aba0de819c49e56614feae5c12f69883f4ae9bb5fad9", size = 56314811 },
            { url = "https://files.pythonhosted.org/packages/8a/6d/44ad094874c6f1b9c654f8ed939590bdc408349f137f9b98a3a23ccec411/nvidia_curand_cu12-10.3.5.147-py3-none-manylinux2014_x86_64.whl", hash = "sha256:a88f583d4e0bb643c49743469964103aa59f7f708d862c3ddb0fc07f851e3b8b", size = 56305206 },
            { url = "https://files.pythonhosted.org/packages/1c/22/2573503d0d4e45673c263a313f79410e110eb562636b0617856fdb2ff5f6/nvidia_curand_cu12-10.3.5.147-py3-none-win_amd64.whl", hash = "sha256:f307cc191f96efe9e8f05a87096abc20d08845a841889ef78cb06924437f6771", size = 55799918 },
        ]

        [[package]]
        name = "nvidia-cusolver-cu12"
        version = "11.6.1.9"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "nvidia-cublas-cu12" },
            { name = "nvidia-cusparse-cu12" },
            { name = "nvidia-nvjitlink-cu12" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/46/6b/a5c33cf16af09166845345275c34ad2190944bcc6026797a39f8e0a282e0/nvidia_cusolver_cu12-11.6.1.9-py3-none-manylinux2014_aarch64.whl", hash = "sha256:d338f155f174f90724bbde3758b7ac375a70ce8e706d70b018dd3375545fc84e", size = 127634111 },
            { url = "https://files.pythonhosted.org/packages/3a/e1/5b9089a4b2a4790dfdea8b3a006052cfecff58139d5a4e34cb1a51df8d6f/nvidia_cusolver_cu12-11.6.1.9-py3-none-manylinux2014_x86_64.whl", hash = "sha256:19e33fa442bcfd085b3086c4ebf7e8debc07cfe01e11513cc6d332fd918ac260", size = 127936057 },
            { url = "https://files.pythonhosted.org/packages/f2/be/d435b7b020e854d5d5a682eb5de4328fd62f6182507406f2818280e206e2/nvidia_cusolver_cu12-11.6.1.9-py3-none-win_amd64.whl", hash = "sha256:e77314c9d7b694fcebc84f58989f3aa4fb4cb442f12ca1a9bde50f5e8f6d1b9c", size = 125224015 },
        ]

        [[package]]
        name = "nvidia-cusparse-cu12"
        version = "12.3.1.170"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "nvidia-nvjitlink-cu12" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/96/a9/c0d2f83a53d40a4a41be14cea6a0bf9e668ffcf8b004bd65633f433050c0/nvidia_cusparse_cu12-12.3.1.170-py3-none-manylinux2014_aarch64.whl", hash = "sha256:9d32f62896231ebe0480efd8a7f702e143c98cfaa0e8a76df3386c1ba2b54df3", size = 207381987 },
            { url = "https://files.pythonhosted.org/packages/db/f7/97a9ea26ed4bbbfc2d470994b8b4f338ef663be97b8f677519ac195e113d/nvidia_cusparse_cu12-12.3.1.170-py3-none-manylinux2014_x86_64.whl", hash = "sha256:ea4f11a2904e2a8dc4b1833cc1b5181cde564edd0d5cd33e3c168eff2d1863f1", size = 207454763 },
            { url = "https://files.pythonhosted.org/packages/a2/e0/3155ca539760a8118ec94cc279b34293309bcd14011fc724f87f31988843/nvidia_cusparse_cu12-12.3.1.170-py3-none-win_amd64.whl", hash = "sha256:9bc90fb087bc7b4c15641521f31c0371e9a612fc2ba12c338d3ae032e6b6797f", size = 204684315 },
        ]

        [[package]]
        name = "nvidia-nccl-cu12"
        version = "2.21.5"
        source = { registry = "https://pypi.org/simple" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/df/99/12cd266d6233f47d00daf3a72739872bdc10267d0383508b0b9c84a18bb6/nvidia_nccl_cu12-2.21.5-py3-none-manylinux2014_x86_64.whl", hash = "sha256:8579076d30a8c24988834445f8d633c697d42397e92ffc3f63fa26766d25e0a0", size = 188654414 },
        ]

        [[package]]
        name = "nvidia-nvjitlink-cu12"
        version = "12.4.127"
        source = { registry = "https://pypi.org/simple" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/02/45/239d52c05074898a80a900f49b1615d81c07fceadd5ad6c4f86a987c0bc4/nvidia_nvjitlink_cu12-12.4.127-py3-none-manylinux2014_aarch64.whl", hash = "sha256:4abe7fef64914ccfa909bc2ba39739670ecc9e820c83ccc7a6ed414122599b83", size = 20552510 },
            { url = "https://files.pythonhosted.org/packages/ff/ff/847841bacfbefc97a00036e0fce5a0f086b640756dc38caea5e1bb002655/nvidia_nvjitlink_cu12-12.4.127-py3-none-manylinux2014_x86_64.whl", hash = "sha256:06b3b9b25bf3f8af351d664978ca26a16d2c5127dbd53c0497e28d1fb9611d57", size = 21066810 },
            { url = "https://files.pythonhosted.org/packages/81/19/0babc919031bee42620257b9a911c528f05fb2688520dcd9ca59159ffea8/nvidia_nvjitlink_cu12-12.4.127-py3-none-win_amd64.whl", hash = "sha256:fd9020c501d27d135f983c6d3e244b197a7ccad769e34df53a42e276b0e25fa1", size = 95336325 },
        ]

        [[package]]
        name = "nvidia-nvtx-cu12"
        version = "12.4.127"
        source = { registry = "https://pypi.org/simple" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/06/39/471f581edbb7804b39e8063d92fc8305bdc7a80ae5c07dbe6ea5c50d14a5/nvidia_nvtx_cu12-12.4.127-py3-none-manylinux2014_aarch64.whl", hash = "sha256:7959ad635db13edf4fc65c06a6e9f9e55fc2f92596db928d169c0bb031e88ef3", size = 100417 },
            { url = "https://files.pythonhosted.org/packages/87/20/199b8713428322a2f22b722c62b8cc278cc53dffa9705d744484b5035ee9/nvidia_nvtx_cu12-12.4.127-py3-none-manylinux2014_x86_64.whl", hash = "sha256:781e950d9b9f60d8241ccea575b32f5105a5baf4c2351cab5256a24869f12a1a", size = 99144 },
            { url = "https://files.pythonhosted.org/packages/54/1b/f77674fbb73af98843be25803bbd3b9a4f0a96c75b8d33a2854a5c7d2d77/nvidia_nvtx_cu12-12.4.127-py3-none-win_amd64.whl", hash = "sha256:641dccaaa1139f3ffb0d3164b4b84f9d253397e38246a4f2f36728b48566d485", size = 66307 },
        ]

        [[package]]
        name = "pillow"
        version = "11.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a5/26/0d95c04c868f6bdb0c447e3ee2de5564411845e36a858cfd63766bc7b563/pillow-11.0.0.tar.gz", hash = "sha256:72bacbaf24ac003fea9bff9837d1eedb6088758d41e100c1552930151f677739", size = 46737780 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/1c/a3/26e606ff0b2daaf120543e537311fa3ae2eb6bf061490e4fea51771540be/pillow-11.0.0-cp312-cp312-macosx_10_13_x86_64.whl", hash = "sha256:d2c0a187a92a1cb5ef2c8ed5412dd8d4334272617f532d4ad4de31e0495bd923", size = 3147642 },
            { url = "https://files.pythonhosted.org/packages/4f/d5/1caabedd8863526a6cfa44ee7a833bd97f945dc1d56824d6d76e11731939/pillow-11.0.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:084a07ef0821cfe4858fe86652fffac8e187b6ae677e9906e192aafcc1b69903", size = 2978999 },
            { url = "https://files.pythonhosted.org/packages/d9/ff/5a45000826a1aa1ac6874b3ec5a856474821a1b59d838c4f6ce2ee518fe9/pillow-11.0.0-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:8069c5179902dcdce0be9bfc8235347fdbac249d23bd90514b7a47a72d9fecf4", size = 4196794 },
            { url = "https://files.pythonhosted.org/packages/9d/21/84c9f287d17180f26263b5f5c8fb201de0f88b1afddf8a2597a5c9fe787f/pillow-11.0.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f02541ef64077f22bf4924f225c0fd1248c168f86e4b7abdedd87d6ebaceab0f", size = 4300762 },
            { url = "https://files.pythonhosted.org/packages/84/39/63fb87cd07cc541438b448b1fed467c4d687ad18aa786a7f8e67b255d1aa/pillow-11.0.0-cp312-cp312-manylinux_2_28_aarch64.whl", hash = "sha256:fcb4621042ac4b7865c179bb972ed0da0218a076dc1820ffc48b1d74c1e37fe9", size = 4210468 },
            { url = "https://files.pythonhosted.org/packages/7f/42/6e0f2c2d5c60f499aa29be14f860dd4539de322cd8fb84ee01553493fb4d/pillow-11.0.0-cp312-cp312-manylinux_2_28_x86_64.whl", hash = "sha256:00177a63030d612148e659b55ba99527803288cea7c75fb05766ab7981a8c1b7", size = 4381824 },
            { url = "https://files.pythonhosted.org/packages/31/69/1ef0fb9d2f8d2d114db982b78ca4eeb9db9a29f7477821e160b8c1253f67/pillow-11.0.0-cp312-cp312-musllinux_1_2_aarch64.whl", hash = "sha256:8853a3bf12afddfdf15f57c4b02d7ded92c7a75a5d7331d19f4f9572a89c17e6", size = 4296436 },
            { url = "https://files.pythonhosted.org/packages/44/ea/dad2818c675c44f6012289a7c4f46068c548768bc6c7f4e8c4ae5bbbc811/pillow-11.0.0-cp312-cp312-musllinux_1_2_x86_64.whl", hash = "sha256:3107c66e43bda25359d5ef446f59c497de2b5ed4c7fdba0894f8d6cf3822dafc", size = 4429714 },
            { url = "https://files.pythonhosted.org/packages/af/3a/da80224a6eb15bba7a0dcb2346e2b686bb9bf98378c0b4353cd88e62b171/pillow-11.0.0-cp312-cp312-win32.whl", hash = "sha256:86510e3f5eca0ab87429dd77fafc04693195eec7fd6a137c389c3eeb4cfb77c6", size = 2249631 },
            { url = "https://files.pythonhosted.org/packages/57/97/73f756c338c1d86bb802ee88c3cab015ad7ce4b838f8a24f16b676b1ac7c/pillow-11.0.0-cp312-cp312-win_amd64.whl", hash = "sha256:8ec4a89295cd6cd4d1058a5e6aec6bf51e0eaaf9714774e1bfac7cfc9051db47", size = 2567533 },
            { url = "https://files.pythonhosted.org/packages/0b/30/2b61876e2722374558b871dfbfcbe4e406626d63f4f6ed92e9c8e24cac37/pillow-11.0.0-cp312-cp312-win_arm64.whl", hash = "sha256:27a7860107500d813fcd203b4ea19b04babe79448268403172782754870dac25", size = 2254890 },
            { url = "https://files.pythonhosted.org/packages/63/24/e2e15e392d00fcf4215907465d8ec2a2f23bcec1481a8ebe4ae760459995/pillow-11.0.0-cp313-cp313-macosx_10_13_x86_64.whl", hash = "sha256:bcd1fb5bb7b07f64c15618c89efcc2cfa3e95f0e3bcdbaf4642509de1942a699", size = 3147300 },
            { url = "https://files.pythonhosted.org/packages/43/72/92ad4afaa2afc233dc44184adff289c2e77e8cd916b3ddb72ac69495bda3/pillow-11.0.0-cp313-cp313-macosx_11_0_arm64.whl", hash = "sha256:0e038b0745997c7dcaae350d35859c9715c71e92ffb7e0f4a8e8a16732150f38", size = 2978742 },
            { url = "https://files.pythonhosted.org/packages/9e/da/c8d69c5bc85d72a8523fe862f05ababdc52c0a755cfe3d362656bb86552b/pillow-11.0.0-cp313-cp313-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:0ae08bd8ffc41aebf578c2af2f9d8749d91f448b3bfd41d7d9ff573d74f2a6b2", size = 4194349 },
            { url = "https://files.pythonhosted.org/packages/cd/e8/686d0caeed6b998351d57796496a70185376ed9c8ec7d99e1d19ad591fc6/pillow-11.0.0-cp313-cp313-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:d69bfd8ec3219ae71bcde1f942b728903cad25fafe3100ba2258b973bd2bc1b2", size = 4298714 },
            { url = "https://files.pythonhosted.org/packages/ec/da/430015cec620d622f06854be67fd2f6721f52fc17fca8ac34b32e2d60739/pillow-11.0.0-cp313-cp313-manylinux_2_28_aarch64.whl", hash = "sha256:61b887f9ddba63ddf62fd02a3ba7add935d053b6dd7d58998c630e6dbade8527", size = 4208514 },
            { url = "https://files.pythonhosted.org/packages/44/ae/7e4f6662a9b1cb5f92b9cc9cab8321c381ffbee309210940e57432a4063a/pillow-11.0.0-cp313-cp313-manylinux_2_28_x86_64.whl", hash = "sha256:c6a660307ca9d4867caa8d9ca2c2658ab685de83792d1876274991adec7b93fa", size = 4380055 },
            { url = "https://files.pythonhosted.org/packages/74/d5/1a807779ac8a0eeed57f2b92a3c32ea1b696e6140c15bd42eaf908a261cd/pillow-11.0.0-cp313-cp313-musllinux_1_2_aarch64.whl", hash = "sha256:73e3a0200cdda995c7e43dd47436c1548f87a30bb27fb871f352a22ab8dcf45f", size = 4296751 },
            { url = "https://files.pythonhosted.org/packages/38/8c/5fa3385163ee7080bc13026d59656267daaaaf3c728c233d530e2c2757c8/pillow-11.0.0-cp313-cp313-musllinux_1_2_x86_64.whl", hash = "sha256:fba162b8872d30fea8c52b258a542c5dfd7b235fb5cb352240c8d63b414013eb", size = 4430378 },
            { url = "https://files.pythonhosted.org/packages/ca/1d/ad9c14811133977ff87035bf426875b93097fb50af747793f013979facdb/pillow-11.0.0-cp313-cp313-win32.whl", hash = "sha256:f1b82c27e89fffc6da125d5eb0ca6e68017faf5efc078128cfaa42cf5cb38798", size = 2249588 },
            { url = "https://files.pythonhosted.org/packages/fb/01/3755ba287dac715e6afdb333cb1f6d69740a7475220b4637b5ce3d78cec2/pillow-11.0.0-cp313-cp313-win_amd64.whl", hash = "sha256:8ba470552b48e5835f1d23ecb936bb7f71d206f9dfeee64245f30c3270b994de", size = 2567509 },
            { url = "https://files.pythonhosted.org/packages/c0/98/2c7d727079b6be1aba82d195767d35fcc2d32204c7a5820f822df5330152/pillow-11.0.0-cp313-cp313-win_arm64.whl", hash = "sha256:846e193e103b41e984ac921b335df59195356ce3f71dcfd155aa79c603873b84", size = 2254791 },
            { url = "https://files.pythonhosted.org/packages/eb/38/998b04cc6f474e78b563716b20eecf42a2fa16a84589d23c8898e64b0ffd/pillow-11.0.0-cp313-cp313t-macosx_10_13_x86_64.whl", hash = "sha256:4ad70c4214f67d7466bea6a08061eba35c01b1b89eaa098040a35272a8efb22b", size = 3150854 },
            { url = "https://files.pythonhosted.org/packages/13/8e/be23a96292113c6cb26b2aa3c8b3681ec62b44ed5c2bd0b258bd59503d3c/pillow-11.0.0-cp313-cp313t-macosx_11_0_arm64.whl", hash = "sha256:6ec0d5af64f2e3d64a165f490d96368bb5dea8b8f9ad04487f9ab60dc4bb6003", size = 2982369 },
            { url = "https://files.pythonhosted.org/packages/97/8a/3db4eaabb7a2ae8203cd3a332a005e4aba00067fc514aaaf3e9721be31f1/pillow-11.0.0-cp313-cp313t-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:c809a70e43c7977c4a42aefd62f0131823ebf7dd73556fa5d5950f5b354087e2", size = 4333703 },
            { url = "https://files.pythonhosted.org/packages/28/ac/629ffc84ff67b9228fe87a97272ab125bbd4dc462745f35f192d37b822f1/pillow-11.0.0-cp313-cp313t-manylinux_2_28_x86_64.whl", hash = "sha256:4b60c9520f7207aaf2e1d94de026682fc227806c6e1f55bba7606d1c94dd623a", size = 4412550 },
            { url = "https://files.pythonhosted.org/packages/d6/07/a505921d36bb2df6868806eaf56ef58699c16c388e378b0dcdb6e5b2fb36/pillow-11.0.0-cp313-cp313t-musllinux_1_2_x86_64.whl", hash = "sha256:1e2688958a840c822279fda0086fec1fdab2f95bf2b717b66871c4ad9859d7e8", size = 4461038 },
            { url = "https://files.pythonhosted.org/packages/d6/b9/fb620dd47fc7cc9678af8f8bd8c772034ca4977237049287e99dda360b66/pillow-11.0.0-cp313-cp313t-win32.whl", hash = "sha256:607bbe123c74e272e381a8d1957083a9463401f7bd01287f50521ecb05a313f8", size = 2253197 },
            { url = "https://files.pythonhosted.org/packages/df/86/25dde85c06c89d7fc5db17940f07aae0a56ac69aa9ccb5eb0f09798862a8/pillow-11.0.0-cp313-cp313t-win_amd64.whl", hash = "sha256:5c39ed17edea3bc69c743a8dd3e9853b7509625c2462532e62baa0732163a904", size = 2572169 },
            { url = "https://files.pythonhosted.org/packages/51/85/9c33f2517add612e17f3381aee7c4072779130c634921a756c97bc29fb49/pillow-11.0.0-cp313-cp313t-win_arm64.whl", hash = "sha256:75acbbeb05b86bc53cbe7b7e6fe00fbcf82ad7c684b3ad82e3d711da9ba287d3", size = 2256828 },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }

        [package.optional-dependencies]
        cpu = [
            { name = "torch", version = "2.5.1", source = { registry = "https://download.pytorch.org/whl/cpu" }, marker = "sys_platform == 'darwin'" },
            { name = "torch", version = "2.5.1+cpu", source = { registry = "https://download.pytorch.org/whl/cpu" }, marker = "sys_platform != 'darwin'" },
            { name = "torchvision", version = "0.20.1", source = { registry = "https://download.pytorch.org/whl/cpu" }, marker = "sys_platform == 'darwin'" },
            { name = "torchvision", version = "0.20.1+cpu", source = { registry = "https://download.pytorch.org/whl/cpu" }, marker = "sys_platform != 'darwin'" },
        ]
        cu124 = [
            { name = "torch", version = "2.5.1+cu124", source = { registry = "https://download.pytorch.org/whl/cu124" } },
            { name = "torchvision", version = "0.20.1+cu124", source = { registry = "https://download.pytorch.org/whl/cu124" } },
        ]

        [package.metadata]
        requires-dist = [
            { name = "torch", marker = "extra == 'cpu'", specifier = ">=2.5.1", index = "https://download.pytorch.org/whl/cpu", conflict = { package = "project", extra = "cpu" } },
            { name = "torch", marker = "extra == 'cu124'", specifier = ">=2.5.1", index = "https://download.pytorch.org/whl/cu124", conflict = { package = "project", extra = "cu124" } },
            { name = "torchvision", marker = "extra == 'cpu'", specifier = ">=0.20.1", index = "https://download.pytorch.org/whl/cpu", conflict = { package = "project", extra = "cpu" } },
            { name = "torchvision", marker = "extra == 'cu124'", specifier = ">=0.20.1", index = "https://download.pytorch.org/whl/cu124", conflict = { package = "project", extra = "cu124" } },
        ]

        [[package]]
        name = "setuptools"
        version = "75.6.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/43/54/292f26c208734e9a7f067aea4a7e282c080750c4546559b58e2e45413ca0/setuptools-75.6.0.tar.gz", hash = "sha256:8199222558df7c86216af4f84c30e9b34a61d8ba19366cc914424cdbd28252f6", size = 1337429 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/21/47d163f615df1d30c094f6c8bbb353619274edccf0327b185cc2493c2c33/setuptools-75.6.0-py3-none-any.whl", hash = "sha256:ce74b49e8f7110f9bf04883b730f4765b774ef3ef28f722cce7c273d253aaf7d", size = 1224032 },
        ]

        [[package]]
        name = "sympy"
        version = "1.13.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "mpmath" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/ca/99/5a5b6f19ff9f083671ddf7b9632028436167cd3d33e11015754e41b249a4/sympy-1.13.1.tar.gz", hash = "sha256:9cebf7e04ff162015ce31c9c6c9144daa34a93bd082f54fd8f12deca4f47515f", size = 7533040 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b2/fe/81695a1aa331a842b582453b605175f419fe8540355886031328089d840a/sympy-1.13.1-py3-none-any.whl", hash = "sha256:db36cdc64bf61b9b24578b6f7bab1ecdd2452cf008f34faa33776680c26d66f8", size = 6189177 },
        ]

        [[package]]
        name = "torch"
        version = "2.5.1"
        source = { registry = "https://download.pytorch.org/whl/cpu" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        dependencies = [
            { name = "filelock", marker = "sys_platform == 'darwin'" },
            { name = "fsspec", marker = "sys_platform == 'darwin'" },
            { name = "jinja2", marker = "sys_platform == 'darwin'" },
            { name = "networkx", marker = "sys_platform == 'darwin'" },
            { name = "setuptools", marker = "sys_platform == 'darwin'" },
            { name = "sympy", marker = "sys_platform == 'darwin'" },
            { name = "typing-extensions", marker = "sys_platform == 'darwin'" },
        ]
        wheels = [
            { url = "https://download.pytorch.org/whl/cpu/torch-2.5.1-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:36d1be99281b6f602d9639bd0af3ee0006e7aab16f6718d86f709d395b6f262c" },
            { url = "https://download.pytorch.org/whl/cpu/torch-2.5.1-cp312-none-macosx_11_0_arm64.whl", hash = "sha256:8c712df61101964eb11910a846514011f0b6f5920c55dbf567bff8a34163d5b1" },
        ]

        [[package]]
        name = "torch"
        version = "2.5.1+cpu"
        source = { registry = "https://download.pytorch.org/whl/cpu" }
        resolution-markers = [
            "sys_platform != 'darwin'",
        ]
        dependencies = [
            { name = "filelock", marker = "sys_platform != 'darwin'" },
            { name = "fsspec", marker = "sys_platform != 'darwin'" },
            { name = "jinja2", marker = "sys_platform != 'darwin'" },
            { name = "networkx", marker = "sys_platform != 'darwin'" },
            { name = "setuptools", marker = "sys_platform != 'darwin'" },
            { name = "sympy", marker = "sys_platform != 'darwin'" },
            { name = "typing-extensions", marker = "sys_platform != 'darwin'" },
        ]
        wheels = [
            { url = "https://download.pytorch.org/whl/cpu/torch-2.5.1%2Bcpu-cp312-cp312-linux_x86_64.whl", hash = "sha256:4856f9d6925121d13c2df07aa7580b767f449dfe71ae5acde9c27535d5da4840" },
            { url = "https://download.pytorch.org/whl/cpu/torch-2.5.1%2Bcpu-cp312-cp312-win_amd64.whl", hash = "sha256:a6b720410350765d3d77c01a5ce098a6c45af446284e45e87a98b8a16e7d564d" },
            { url = "https://download.pytorch.org/whl/cpu/torch-2.5.1%2Bcpu-cp313-cp313-linux_x86_64.whl", hash = "sha256:5dbbdf83caa90d0bcaa50e4933ca424889133b35226db79000877d4ec5d9ea37" },
        ]

        [[package]]
        name = "torch"
        version = "2.5.1+cu124"
        source = { registry = "https://download.pytorch.org/whl/cu124" }
        dependencies = [
            { name = "filelock" },
            { name = "fsspec" },
            { name = "jinja2" },
            { name = "networkx" },
            { name = "nvidia-cublas-cu12", marker = "platform_machine == 'x86_64' and sys_platform == 'linux'" },
            { name = "nvidia-cuda-cupti-cu12", marker = "platform_machine == 'x86_64' and sys_platform == 'linux'" },
            { name = "nvidia-cuda-nvrtc-cu12", marker = "platform_machine == 'x86_64' and sys_platform == 'linux'" },
            { name = "nvidia-cuda-runtime-cu12", marker = "platform_machine == 'x86_64' and sys_platform == 'linux'" },
            { name = "nvidia-cudnn-cu12", marker = "platform_machine == 'x86_64' and sys_platform == 'linux'" },
            { name = "nvidia-cufft-cu12", marker = "platform_machine == 'x86_64' and sys_platform == 'linux'" },
            { name = "nvidia-curand-cu12", marker = "platform_machine == 'x86_64' and sys_platform == 'linux'" },
            { name = "nvidia-cusolver-cu12", marker = "platform_machine == 'x86_64' and sys_platform == 'linux'" },
            { name = "nvidia-cusparse-cu12", marker = "platform_machine == 'x86_64' and sys_platform == 'linux'" },
            { name = "nvidia-nccl-cu12", marker = "platform_machine == 'x86_64' and sys_platform == 'linux'" },
            { name = "nvidia-nvjitlink-cu12", marker = "platform_machine == 'x86_64' and sys_platform == 'linux'" },
            { name = "nvidia-nvtx-cu12", marker = "platform_machine == 'x86_64' and sys_platform == 'linux'" },
            { name = "setuptools" },
            { name = "sympy" },
            { name = "triton", marker = "python_full_version < '3.13' and platform_machine == 'x86_64' and sys_platform == 'linux'" },
            { name = "typing-extensions" },
        ]
        wheels = [
            { url = "https://download.pytorch.org/whl/cu124/torch-2.5.1%2Bcu124-cp312-cp312-linux_x86_64.whl", hash = "sha256:bf6484bfe5bc4f92a4a1a1bf553041505e19a911f717065330eb061afe0e14d7" },
            { url = "https://download.pytorch.org/whl/cu124/torch-2.5.1%2Bcu124-cp312-cp312-win_amd64.whl", hash = "sha256:3c3f705fb125edbd77f9579fa11a138c56af8968a10fc95834cdd9fdf4f1f1a6" },
            { url = "https://download.pytorch.org/whl/cu124/torch-2.5.1%2Bcu124-cp313-cp313-linux_x86_64.whl", hash = "sha256:e9bebf91ede89267577911da4b0709ac6113a0cff6a1c2202c046b1ec2a51601" },
        ]

        [[package]]
        name = "torchvision"
        version = "0.20.1"
        source = { registry = "https://download.pytorch.org/whl/cpu" }
        resolution-markers = [
            "sys_platform == 'darwin'",
        ]
        dependencies = [
            { name = "numpy", marker = "sys_platform == 'darwin'" },
            { name = "pillow", marker = "sys_platform == 'darwin'" },
            { name = "torch", version = "2.5.1", source = { registry = "https://download.pytorch.org/whl/cpu" }, marker = "sys_platform == 'darwin'" },
        ]
        wheels = [
            { url = "https://download.pytorch.org/whl/cpu/torchvision-0.20.1-cp312-cp312-linux_aarch64.whl", hash = "sha256:9f853ba4497ac4691815ad41b523ee23cf5ba4f87b1ce869d704052e233ca8b7" },
            { url = "https://download.pytorch.org/whl/cpu/torchvision-0.20.1-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:1a31256ff945d64f006bb306813a7c95a531fe16bfb2535c837dd4c104533d7a" },
        ]

        [[package]]
        name = "torchvision"
        version = "0.20.1+cpu"
        source = { registry = "https://download.pytorch.org/whl/cpu" }
        resolution-markers = [
            "sys_platform != 'darwin'",
        ]
        dependencies = [
            { name = "numpy", marker = "sys_platform != 'darwin'" },
            { name = "pillow", marker = "sys_platform != 'darwin'" },
            { name = "torch", version = "2.5.1+cpu", source = { registry = "https://download.pytorch.org/whl/cpu" }, marker = "sys_platform != 'darwin'" },
        ]
        wheels = [
            { url = "https://download.pytorch.org/whl/cpu/torchvision-0.20.1%2Bcpu-cp312-cp312-linux_x86_64.whl", hash = "sha256:5f46c7ac7f00a065cb40bfb1e1bfc4ba16a35f5d46b3fe70cca6b3cea7f822f7" },
            { url = "https://download.pytorch.org/whl/cpu/torchvision-0.20.1%2Bcpu-cp312-cp312-win_amd64.whl", hash = "sha256:64832095b9b8b8185f24788f1a531ae96c937bf797cd3474a2ea7511852e5cd7" },
        ]

        [[package]]
        name = "torchvision"
        version = "0.20.1+cu124"
        source = { registry = "https://download.pytorch.org/whl/cu124" }
        dependencies = [
            { name = "numpy" },
            { name = "pillow" },
            { name = "torch", version = "2.5.1+cu124", source = { registry = "https://download.pytorch.org/whl/cu124" } },
        ]
        wheels = [
            { url = "https://download.pytorch.org/whl/cu124/torchvision-0.20.1%2Bcu124-cp312-cp312-linux_x86_64.whl", hash = "sha256:d1053ec5054549e7dac2613b151bffe323f3c924939d296df4d7d34925aaf3ad" },
            { url = "https://download.pytorch.org/whl/cu124/torchvision-0.20.1%2Bcu124-cp312-cp312-win_amd64.whl", hash = "sha256:0f6c7b3b0e13663fb3359e64f3604c0ab74c2b4809ae6949ace5635a5240f0e5" },
        ]

        [[package]]
        name = "triton"
        version = "3.1.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "filelock" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/78/eb/65f5ba83c2a123f6498a3097746607e5b2f16add29e36765305e4ac7fdd8/triton-3.1.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:c8182f42fd8080a7d39d666814fa36c5e30cc00ea7eeeb1a2983dbb4c99a0fdc", size = 209551444 },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.12.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/df/db/f35a00659bc03fec321ba8bce9420de607a1d37f8342eee1863174c69557/typing_extensions-4.12.2.tar.gz", hash = "sha256:1a7ead55c7e559dd4dee8856e3a88b41225abfe1ce8df57b7c13915fe121ffb8", size = 85321 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/26/9f/ad63fc0248c5379346306f8668cda6e2e2e9c95e01216d2b8ffd9ff037d0/typing_extensions-4.12.2-py3-none-any.whl", hash = "sha256:04e5ca0351e0f3f85c6853954072df659d0d13fac324d0072316b67d7794700d", size = 37438 },
        ]
        "###
        );
    });

    Ok(())
}

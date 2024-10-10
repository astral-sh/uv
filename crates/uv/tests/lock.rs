#![cfg(all(feature = "python", feature = "pypi"))]

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::*;
use indoc::{formatdoc, indoc};
use insta::assert_snapshot;
use std::io::BufReader;
use url::Url;

use common::{uv_snapshot, TestContext};
use uv_fs::Simplified;

use crate::common::{build_vendor_links_url, decode_token, packse_index_url};

mod common;

/// Lock a requirement from PyPI.
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

    uv_snapshot!(context.filters(), context.lock().env_remove("UV_EXCLUDE_NEWER"), @r###"
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
    uv_snapshot!(context.filters(), context.lock().arg("--locked").env_remove("UV_EXCLUDE_NEWER"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").env_remove("UV_EXCLUDE_NEWER"), @r###"
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
            { name = "uvloop", marker = "platform_python_implementation == 'CPython' and platform_system != 'Windows' and extra == 'test'", specifier = ">=0.17" },
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
    error: failed to generate package metadata for `anyio==4.3.0 @ direct+https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl`
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
            { name = "uvloop", marker = "platform_python_implementation == 'CPython' and platform_system != 'Windows' and extra == 'test'", specifier = ">=0.17" },
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
            { name = "colorama", marker = "platform_system == 'Windows'" },
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
    Resolved 7 packages in [TIME]
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
            "python_full_version < '3.10'",
            "python_full_version >= '3.10'",
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
            { name = "urllib3" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/af/47/b215df9f71b4fdba1025fc05a77db2ad243fa0926755a52c5e71659f4e3c/urllib3-2.0.7.tar.gz", hash = "sha256:c97dfde1f7bd43a71c8d2a58e369e9b2bf692d1334ea9f9cae55add7d0dd0f84", size = 282546 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d2/b2/b157855192a68541a91ba7b2bbcb91f1b4faa51f8bae38d8005c034be524/urllib3-2.0.7-py3-none-any.whl", hash = "sha256:fdb6d215c776278489906c2f8916e6e7d4f5a9b602ccbcfdf7f016fc8da0596e", size = 124213 },
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
     + urllib3==2.0.7
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
    Resolved 7 packages in [TIME]
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
     + urllib3==2.0.7
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
            { name = "colorama", marker = "platform_system == 'Windows'" },
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

    // Lock against `main`.
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
      × No solution found when resolving dependencies:
      ╰─▶ Because the requested Python version (>=3.7) does not satisfy Python>=3.8 and the requested Python version (>=3.7) does not satisfy Python>=3.7.9,<3.8, we can conclude that Python>=3.7.9 is incompatible.
          And because pygls>=1.1.0,<=1.2.1 depends on Python>=3.7.9,<4 and only pygls<=1.3.0 is available, we can conclude that all of:
              pygls>=1.1.0,<1.3.0
              pygls>1.3.0
           cannot be used. (1)

          Because the requested Python version (>=3.7) does not satisfy Python>=3.8 and pygls==1.3.0 depends on Python>=3.8, we can conclude that pygls==1.3.0 cannot be used.
          And because we know from (1) that all of:
              pygls>=1.1.0,<1.3.0
              pygls>1.3.0
           cannot be used, we can conclude that pygls>=1.1.0 cannot be used.
          And because your project depends on pygls>=1.1.0, we can conclude that your project's requirements are unsatisfiable.

          hint: Pre-releases are available for pygls in the requested range (e.g., 2.0.0a1), but pre-releases weren't enabled (try: `--prerelease=allow`)

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
    Resolved 10 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(&lockfile).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.7"

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
        dependencies = [
            { name = "attrs" },
            { name = "exceptiongroup", marker = "python_full_version < '3.11'" },
            { name = "typing-extensions", marker = "python_full_version < '3.11'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/68/d4/27f9fd840e74d51b6d6a024d39ff495b56ffde71d28eb82758b7b85d0617/cattrs-23.1.2.tar.gz", hash = "sha256:db1c821b8c537382b2c7c66678c3790091ca0275ac486c76f3c8f3920e83c657", size = 39998 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/3a/ba/05df14efaa0624fac6b1510e87f5ce446208d2f6ce50270a89b6268aebfe/cattrs-23.1.2-py3-none-any.whl", hash = "sha256:b2bb14311ac17bed0d58785e5a60f022e5431aca3932e3fc5cc8ed8639de50a4", size = 50845 },
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
            { name = "typing-extensions", marker = "python_full_version < '3.8'" },
            { name = "zipp" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/a3/82/f6e29c8d5c098b6be61460371c2c5591f4a335923639edec43b3830650a4/importlib_metadata-6.7.0.tar.gz", hash = "sha256:1aaf550d4f73e5d6783e7acb77aec43d49da8017410afae93822cc9cca98c4d4", size = 53569 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ff/94/64287b38c7de4c90683630338cf28f129decbba0a44f0c6db35a873c73c4/importlib_metadata-6.7.0-py3-none-any.whl", hash = "sha256:cb52082e659e97afc5dac71e79de97d8681de3aa07ff18578330904a9d18e5b5", size = 22934 },
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
        version = "1.0.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "lsprotocol" },
            { name = "typeguard" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/8e/27/58ff0f76b379fc11a1d03e8d4b4e96fd0abb463d27709a7fb4193bcdbbc4/pygls-1.0.1.tar.gz", hash = "sha256:f3ee98ddbb4690eb5c755bc32ba7e129607f14cbd313575f33d0cea443b78cb2", size = 674546 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/da/9b/4fd77a060068f2f3f46f97ed6ba8762c5a73f11ef0c196cfd34f3a9be878/pygls-1.0.1-py3-none-any.whl", hash = "sha256:adacc96da77598c70f46acfdfd1481d3da90cd54f639f7eee52eb6e4dbd57b55", size = 40367 },
        ]

        [[package]]
        name = "typeguard"
        version = "2.13.3"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/3a/38/c61bfcf62a7b572b5e9363a802ff92559cb427ee963048e1442e3aef7490/typeguard-2.13.3.tar.gz", hash = "sha256:00edaa8da3a133674796cf5ea87d9f4b4c367d77476e185e80251cc13dfbb8c4", size = 40604 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/9a/bb/d43e5c75054e53efce310e79d63df0ac3f25e34c926be5dffb7d283fb2a8/typeguard-2.13.3-py3-none-any.whl", hash = "sha256:5e3e3be01e887e7eafae5af63d1f36c849aaa94e3a0112097312aabfa16284f1", size = 17605 },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.7.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/3c/8b/0111dd7d6c1478bf83baa1cab85c686426c7a6274119aceb2bd9d35395ad/typing_extensions-4.7.1.tar.gz", hash = "sha256:b75ddc264f0ba5615db7ba217daeb99701ad295353c45f9e95963337ceeeffb2", size = 72876 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ec/6b/63cc3df74987c36fe26157ee12e09e8f9db4de771e0f3404263117e75b95/typing_extensions-4.7.1-py3-none-any.whl", hash = "sha256:440d5dd3af93b060174bf433bccd69b0babc3b15b1a8dca43789fd7f61514b36", size = 33232 },
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
    Resolved 9 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(&lockfile).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.7.9"

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
        dependencies = [
            { name = "attrs" },
            { name = "exceptiongroup", marker = "python_full_version < '3.11'" },
            { name = "typing-extensions", marker = "python_full_version < '3.11'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/68/d4/27f9fd840e74d51b6d6a024d39ff495b56ffde71d28eb82758b7b85d0617/cattrs-23.1.2.tar.gz", hash = "sha256:db1c821b8c537382b2c7c66678c3790091ca0275ac486c76f3c8f3920e83c657", size = 39998 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/3a/ba/05df14efaa0624fac6b1510e87f5ce446208d2f6ce50270a89b6268aebfe/cattrs-23.1.2-py3-none-any.whl", hash = "sha256:b2bb14311ac17bed0d58785e5a60f022e5431aca3932e3fc5cc8ed8639de50a4", size = 50845 },
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
            { name = "typing-extensions", marker = "python_full_version < '3.8'" },
            { name = "zipp" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/a3/82/f6e29c8d5c098b6be61460371c2c5591f4a335923639edec43b3830650a4/importlib_metadata-6.7.0.tar.gz", hash = "sha256:1aaf550d4f73e5d6783e7acb77aec43d49da8017410afae93822cc9cca98c4d4", size = 53569 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ff/94/64287b38c7de4c90683630338cf28f129decbba0a44f0c6db35a873c73c4/importlib_metadata-6.7.0-py3-none-any.whl", hash = "sha256:cb52082e659e97afc5dac71e79de97d8681de3aa07ff18578330904a9d18e5b5", size = 22934 },
        ]

        [[package]]
        name = "lsprotocol"
        version = "2023.0.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "attrs" },
            { name = "cattrs" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/3e/fe/f7671a4fb28606ff1663bba60aff6af21b1e43a977c74c33db13cb83680f/lsprotocol-2023.0.0.tar.gz", hash = "sha256:c9d92e12a3f4ed9317d3068226592860aab5357d93cf5b2451dc244eee8f35f2", size = 69399 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2d/5b/f18eb1823a4cee9bed70cdcc25eed5a75845367c42e63a79010a7c34f8a7/lsprotocol-2023.0.0-py3-none-any.whl", hash = "sha256:e85fc87ee26c816adca9eb497bb3db1a7c79c477a11563626e712eaccf926a05", size = 70789 },
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
        version = "1.2.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "lsprotocol" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/e6/94/534c11ba5475df09542e48d751a66e0448d52bbbb92cbef5541deef7760d/pygls-1.2.1.tar.gz", hash = "sha256:04f9b9c115b622dcc346fb390289066565343d60245a424eca77cb429b911ed8", size = 45274 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/36/31/3799444d3f072ffca1a35eb02a48f964384cc13f001125e87d9f0748687b/pygls-1.2.1-py3-none-any.whl", hash = "sha256:7dcfcf12b6f15beb606afa46de2ed348b65a279c340ef2242a9a35c22eeafe94", size = 55983 },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.7.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/3c/8b/0111dd7d6c1478bf83baa1cab85c686426c7a6274119aceb2bd9d35395ad/typing_extensions-4.7.1.tar.gz", hash = "sha256:b75ddc264f0ba5615db7ba217daeb99701ad295353c45f9e95963337ceeeffb2", size = 72876 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ec/6b/63cc3df74987c36fe26157ee12e09e8f9db4de771e0f3404263117e75b95/typing_extensions-4.7.1-py3-none-any.whl", hash = "sha256:440d5dd3af93b060174bf433bccd69b0babc3b15b1a8dca43789fd7f61514b36", size = 33232 },
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

    uv_snapshot!(context.filters(), context.lock().env("UV_EXCLUDE_NEWER", "2024-08-29T00:00:00Z"), @r###"
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
    uv_snapshot!(context.filters(), context.lock().arg("--locked").env("UV_EXCLUDE_NEWER", "2024-08-29T00:00:00Z"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
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
    warning: The workspace `requires-python` value does not contain a lower bound: `<=3.12`. Set a lower bound to indicate the minimum compatible Python version (e.g., `>=3.11`).
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(&lockfile).unwrap();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = "<=3.12"

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

    // Re-run with `--locked`.
    uv_snapshot!(context.filters(), context.lock().arg("--locked"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: The workspace `requires-python` value does not contain a lower bound: `<=3.12`. Set a lower bound to indicate the minimum compatible Python version (e.g., `>=3.11`).
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
            "python_full_version < '3.10'",
            "python_full_version == '3.10'",
            "python_full_version > '3.10' and python_full_version < '3.11'",
            "python_full_version >= '3.11'",
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
    uv_snapshot!(context.filters(), context.lock().arg("--locked").env_remove("UV_EXCLUDE_NEWER"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided. To update the lockfile, run `uv lock`.
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").env_remove("UV_EXCLUDE_NEWER"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to prepare distributions
      Caused by: Failed to fetch wheel: idna==3.6
      Caused by: Hash mismatch for `idna==3.6`

    Expected:
      sha256:aecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca
      sha256:d05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f

    Computed:
      sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f
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
      ╰─▶ Because dearpygui==1.9.1 has no wheels with a matching Python ABI tag and your project depends on dearpygui==1.9.1, we can conclude that your project's requirements are unsatisfiable.
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
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to prepare distributions
      Caused by: Failed to fetch wheel: iniconfig==2.0.0
      Caused by: HTTP status client error (401 Unauthorized) for url (https://pypi-proxy.fly.dev/basic-auth/files/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl)
    "###);

    // Installing from the lockfile should fail without an index.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--no-install-project"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to prepare distributions
      Caused by: Failed to fetch wheel: iniconfig==2.0.0
      Caused by: HTTP status client error (401 Unauthorized) for url (https://pypi-proxy.fly.dev/basic-auth/files/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl)
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
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to prepare distributions
      Caused by: Failed to fetch wheel: iniconfig==2.0.0
      Caused by: HTTP status client error (401 Unauthorized) for url (https://pypi-proxy.fly.dev/basic-auth/files/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl)
    "###);

    // Installing with credentials from with `UV_INDEX_URL` should succeed.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--reinstall").arg("--no-cache").env("UV_INDEX_URL", "https://public:heron@pypi-proxy.fly.dev/basic-auth/simple"), @r###"
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

/// However, we don't currently avoid persisting Git credentials in `uv.lock`.
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
        .env_remove("UV_EXCLUDE_NEWER")
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
        .env_remove("UV_EXCLUDE_NEWER")
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
    warning: The transitive dependency `packaging` is unpinned. Consider setting a lower bound with a constraint when using `--resolution-strategy lowest` to avoid using outdated versions.
    warning: The transitive dependency `colorama` is unpinned. Consider setting a lower bound with a constraint when using `--resolution-strategy lowest` to avoid using outdated versions.
    warning: The transitive dependency `iniconfig` is unpinned. Consider setting a lower bound with a constraint when using `--resolution-strategy lowest` to avoid using outdated versions.
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
        Url::from_directory_path(&root).unwrap().as_str()
    })?;

    uv_snapshot!(context.filters(), context.lock().env_remove("UV_EXCLUDE_NEWER"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");

    let index = Url::from_directory_path(&root).unwrap().to_string();
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
    uv_snapshot!(context.filters(), context.lock().arg("--locked").env_remove("UV_EXCLUDE_NEWER"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    "###);

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").env_remove("UV_EXCLUDE_NEWER"), @r###"
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
    let response =
        reqwest::blocking::get("https://github.com/user-attachments/files/16592193/workspace.zip")?;
    let workspace_archive = context.temp_dir.child("workspace.zip");
    let mut workspace_archive_file = fs_err::File::create(&*workspace_archive)?;
    std::io::copy(&mut response.bytes()?.as_ref(), &mut workspace_archive_file)?;

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
    let response =
        reqwest::blocking::get("https://github.com/user-attachments/files/16592193/workspace.zip")?;
    let workspace_archive = context.temp_dir.child("workspace.zip");
    let mut workspace_archive_file = fs_err::File::create(&*workspace_archive)?;
    std::io::copy(&mut response.bytes()?.as_ref(), &mut workspace_archive_file)?;

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
/// we raise an error.
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
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to build: `project @ file://[TEMP_DIR]/`
      Caused by: Failed to parse entry for: `uv-public-pypackage`
      Caused by: Can't combine URLs from both `project.dependencies` and `tool.uv.sources`
    "###);

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
            "python_full_version < '3.9'",
            "python_full_version >= '3.9' and python_full_version < '3.11'",
            "python_full_version >= '3.11'",
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
            "platform_system != 'Windows'",
        ]
        supported-markers = [
            "platform_system != 'Windows'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "black"
        version = "24.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "click", marker = "platform_system != 'Windows'" },
            { name = "mypy-extensions", marker = "platform_system != 'Windows'" },
            { name = "packaging", marker = "platform_system != 'Windows'" },
            { name = "pathspec", marker = "platform_system != 'Windows'" },
            { name = "platformdirs", marker = "platform_system != 'Windows'" },
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
            { name = "black", marker = "platform_system != 'Windows'" },
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
            { name = "colorama", marker = "platform_system == 'Windows'" },
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
            "platform_system != 'Windows'",
        ]
        supported-markers = [
            "platform_system != 'Windows'",
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
            { name = "click", marker = "platform_system != 'Windows'" },
            { name = "mypy-extensions", marker = "platform_system != 'Windows'" },
            { name = "packaging", marker = "platform_system != 'Windows'" },
            { name = "pathspec", marker = "platform_system != 'Windows'" },
            { name = "platformdirs", marker = "platform_system != 'Windows'" },
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
            { name = "black", marker = "platform_system != 'Windows'" },
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
    error: Supported environments must be disjoint, but the following markers overlap: `platform_system != 'Windows'` and `python_full_version >= '3.11'`.

    hint: replace `python_full_version >= '3.11'` with `python_full_version >= '3.11' and platform_system == 'Windows'`.
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
            "python_full_version < '3.11'",
            "python_full_version >= '3.11'",
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
            { name = "colorama", marker = "platform_system == 'Windows'" },
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
            { name = "colorama", marker = "platform_system == 'Windows'" },
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
            "python_full_version >= '3.13'",
            "python_full_version < '3.9'",
            "python_full_version >= '3.9' and python_full_version < '3.13'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "colorama", marker = "platform_system == 'Windows'" },
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
            "python_full_version >= '3.13'",
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
            { name = "numpy", version = "1.24.4", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version < '3.9' or python_full_version >= '3.13'" },
            { name = "numpy", version = "1.26.4", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version >= '3.9' and python_full_version < '3.13'" },
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
            { name = "colorama", marker = "platform_system == 'Windows'" },
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
    Prepared 2 packages in [TIME]
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
    error: The Python request from `.python-version` resolved to Python 3.12.[X], which is incompatible with the project's Python requirement: `>=3.8, <=3.10`
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
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    error: Failed to build: `b @ file://[TEMP_DIR]/b`
      Caused by: Failed to extract static metadata from `pyproject.toml`
      Caused by: `pyproject.toml` is using the `[project]` table, but the required `project.name` field is not set.
      Caused by: TOML parse error at line 2, column 10
      |
    2 |         [project.urls]
      |          ^^^^^^^
    missing field `name`

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
            "python_full_version < '3.13'",
            "python_full_version >= '3.13'",
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
            "python_full_version < '3.12'",
            "python_full_version == '3.12.*'",
            "python_full_version >= '3.13'",
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

/// Multiple `index` entries is not yet supported.
#[test]
fn lock_multiple_sources_index() -> Result<()> {
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
            { index = "pytorch", marker = "sys_platform != 'win32'" },
            { index = "internal", marker = "sys_platform == 'win32'" },
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
    Sources can only include a single index source

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
        dependencies = ["iniconfig ; platform_system == 'Windows'"]

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
            "platform_system == 'Windows' and sys_platform == 'darwin'",
            "platform_system == 'Windows' and sys_platform != 'darwin'",
            "platform_system != 'Windows'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        resolution-markers = [
            "platform_system == 'Windows' and sys_platform != 'darwin'",
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
            "platform_system == 'Windows' and sys_platform == 'darwin'",
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "iniconfig", version = "2.0.0", source = { registry = "https://pypi.org/simple" }, marker = "platform_system == 'Windows' and sys_platform != 'darwin'" },
            { name = "iniconfig", version = "2.0.0", source = { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl" }, marker = "platform_system == 'Windows' and sys_platform == 'darwin'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "iniconfig", marker = "platform_system == 'Windows' and sys_platform != 'darwin'" },
            { name = "iniconfig", marker = "platform_system == 'Windows' and sys_platform == 'darwin'", url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl" },
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

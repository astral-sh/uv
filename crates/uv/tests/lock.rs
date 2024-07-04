#![cfg(all(feature = "python", feature = "pypi"))]

use anyhow::Result;
use assert_fs::prelude::*;
use indoc::{formatdoc, indoc};
use insta::assert_snapshot;

use common::{uv_snapshot, TestContext};

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
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 4 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [[distribution]]
        name = "anyio"
        version = "3.7.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/c6/b3/fefbf7e78ab3b805dec67d698dc18dd505af7a18a8dd08868c9b4fa736b5/anyio-3.7.0.tar.gz", hash = "sha256:275d9973793619a5374e1c89a4f4ad3f4b0a5510a2b5b939444bee8f4c4d37ce", size = 142737 }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/fe/7ce1926952c8a403b35029e194555558514b365ad77d75125f521a2bec62/anyio-3.7.0-py3-none-any.whl", hash = "sha256:eddca883c4175f14df8aedce21054bfca3adb70ffe76a9f607aef9d7fa2ea7f0", size = 80873 },
        ]

        [[distribution]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio" },
        ]

        [[distribution]]
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

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning.
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
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().env_remove("UV_EXCLUDE_NEWER"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "source-distribution" },
        ]

        [[distribution]]
        name = "source-distribution"
        version = "0.0.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz", hash = "sha256:1f83ed7498336c7f2ab9b002cf22583d91115ebc624053dc4eb3a45694490106", size = 2157 }
        "###
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().env_remove("UV_EXCLUDE_NEWER"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning.
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + source-distribution==0.0.1
    "###);

    Ok(())
}

/// Lock a Git requirement.
#[test]
fn lock_sdist_git() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@0.0.1"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "uv-public-pypackage" },
        ]

        [[distribution]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = { git = "https://github.com/astral-test/uv-public-pypackage?rev=0.0.1#0dacfd662c64cb4ceb16e6cf65a157a8b715b979" }
        "###
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning.
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
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 4 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [[distribution]]
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

        [[distribution]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio" },
        ]

        [[distribution]]
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

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning.
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
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 4 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [[distribution]]
        name = "anyio"
        version = "4.3.0"
        source = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz" }
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]

        [[distribution]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio" },
        ]

        [[distribution]]
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

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning.
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

        [project.optional-dependencies]
        test = ["iniconfig"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 5 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [[distribution]]
        name = "anyio"
        version = "3.7.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/c6/b3/fefbf7e78ab3b805dec67d698dc18dd505af7a18a8dd08868c9b4fa736b5/anyio-3.7.0.tar.gz", hash = "sha256:275d9973793619a5374e1c89a4f4ad3f4b0a5510a2b5b939444bee8f4c4d37ce", size = 142737 }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/fe/7ce1926952c8a403b35029e194555558514b365ad77d75125f521a2bec62/anyio-3.7.0-py3-none-any.whl", hash = "sha256:eddca883c4175f14df8aedce21054bfca3adb70ffe76a9f607aef9d7fa2ea7f0", size = 80873 },
        ]

        [[distribution]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[distribution]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "anyio" },
        ]

        [distribution.optional-dependencies]
        test = [
            { name = "iniconfig" },
        ]

        [[distribution]]
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

    // Install the base dependencies from the lockfile.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning.
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.1
    "###);

    // Install the extras from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--extra").arg("test"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning.
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

        [tool.uv]
        override-dependencies = ["werkzeug==2.3.8"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 9 packages in [TIME]
    "###);

    // Install the base dependencies from the lockfile.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning.
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
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 10 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [[distribution]]
        name = "blinker"
        version = "1.7.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a1/13/6df5fc090ff4e5d246baf1f45fe9e5623aa8565757dfa5bd243f6a545f9e/blinker-1.7.0.tar.gz", hash = "sha256:e6820ff6fa4e4d1d8e2747c2283749c3f547e4fee112b98555cdcdae32996182", size = 28134 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/fa/2a/7f3714cbc6356a0efec525ce7a0613d581072ed6eb53eb7b9754f33db807/blinker-1.7.0-py3-none-any.whl", hash = "sha256:c3f865d4d54db7abc53758a01601cf343fe55b84c1de4e3fa910e420b438d5b9", size = 13068 },
        ]

        [[distribution]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121 }
        dependencies = [
            { name = "colorama", marker = "platform_system == 'Windows'" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941 },
        ]

        [[distribution]]
        name = "colorama"
        version = "0.4.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d8/53/6f443c9a4a8358a93a6792e2acffb9d9d5cb0a5cfd8802644b7b1c9a02e4/colorama-0.4.6.tar.gz", hash = "sha256:08695f5cb7ed6e0531a20572697297273c47b8cae5a63ffc6d6ed5c201be6e44", size = 27697 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d1/d6/3965ed04c63042e047cb6a3e6ed1a63a35087b6a609aa3a15ed8ac56c221/colorama-0.4.6-py2.py3-none-any.whl", hash = "sha256:4f1d9991f5acc0ca119f9d443620b77f9d6b33703e51011c16baf57afb285fc6", size = 25335 },
        ]

        [[distribution]]
        name = "flask"
        version = "3.0.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/3f/e0/a89e8120faea1edbfca1a9b171cff7f2bf62ec860bbafcb2c2387c0317be/flask-3.0.2.tar.gz", hash = "sha256:822c03f4b799204250a7ee84b1eddc40665395333973dfb9deebfe425fefcb7d", size = 675248 }
        dependencies = [
            { name = "blinker" },
            { name = "click" },
            { name = "itsdangerous" },
            { name = "jinja2" },
            { name = "werkzeug" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/93/a6/aa98bfe0eb9b8b15d36cdfd03c8ca86a03968a87f27ce224fb4f766acb23/flask-3.0.2-py3-none-any.whl", hash = "sha256:3232e0e9c850d781933cf0207523d1ece087eb8d87b23777ae38456e2fbe7c6e", size = 101300 },
        ]

        [distribution.optional-dependencies]
        dotenv = [
            { name = "python-dotenv" },
        ]

        [[distribution]]
        name = "itsdangerous"
        version = "2.1.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/7f/a1/d3fb83e7a61fa0c0d3d08ad0a94ddbeff3731c05212617dff3a94e097f08/itsdangerous-2.1.2.tar.gz", hash = "sha256:5dbbc68b317e5e42f327f9021763545dc3fc3bfe22e6deb96aaf1fc38874156a", size = 56143 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/5f/447e04e828f47465eeab35b5d408b7ebaaaee207f48b7136c5a7267a30ae/itsdangerous-2.1.2-py3-none-any.whl", hash = "sha256:2c2349112351b88699d8d4b6b075022c0808887cb7ad10069318a8b0bc88db44", size = 15749 },
        ]

        [[distribution]]
        name = "jinja2"
        version = "3.1.3"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/b2/5e/3a21abf3cd467d7876045335e681d276ac32492febe6d98ad89562d1a7e1/Jinja2-3.1.3.tar.gz", hash = "sha256:ac8bd6544d4bb2c9792bf3a159e80bba8fda7f07e81bc3aed565432d5925ba90", size = 268261 }
        dependencies = [
            { name = "markupsafe" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/30/6d/6de6be2d02603ab56e72997708809e8a5b0fbfee080735109b40a3564843/Jinja2-3.1.3-py3-none-any.whl", hash = "sha256:7d6d50dd97d52cbc355597bd845fabfbac3f551e1f99619e39a35ce8c370b5fa", size = 133236 },
        ]

        [[distribution]]
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

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "flask" },
            { name = "flask", extra = "dotenv" },
        ]

        [[distribution]]
        name = "python-dotenv"
        version = "1.0.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bc/57/e84d88dfe0aec03b7a2d4327012c1627ab5f03652216c63d49846d7a6c58/python-dotenv-1.0.1.tar.gz", hash = "sha256:e324ee90a023d808f1959c46bcbc04446a10ced277783dc6ee09987c37ec10ca", size = 39115 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/6a/3e/b68c118422ec867fa7ab88444e1274aa40681c606d59ac27de5a5588f082/python_dotenv-1.0.1-py3-none-any.whl", hash = "sha256:f7b63ef50f1b690dddf550d03497b66d609393b40b564ed0d674909a68ebf16a", size = 19863 },
        ]

        [[distribution]]
        name = "werkzeug"
        version = "3.0.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/0d/cc/ff1904eb5eb4b455e442834dabf9427331ac0fa02853bf83db817a7dd53d/werkzeug-3.0.1.tar.gz", hash = "sha256:507e811ecea72b18a404947aded4b3390e1db8f826b494d76550ef45bb3b1dcc", size = 801436 }
        dependencies = [
            { name = "markupsafe" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c3/fc/254c3e9b5feb89ff5b9076a23218dafbc99c96ac5941e900b71206e6313b/werkzeug-3.0.1-py3-none-any.whl", hash = "sha256:90a285dc0e42ad56b34e696398b8122ee4c681833fb35b8334a095d82c56da10", size = 226669 },
        ]
        "###
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning.
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
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 7 packages in [TIME]
    "###);

    let lockfile = context.temp_dir.join("uv.lock");
    let lock = fs_err::read_to_string(&lockfile)?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.7"

        [[distribution]]
        name = "certifi"
        version = "2024.2.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/71/da/e94e26401b62acd6d91df2b52954aceb7f561743aa5ccc32152886c76c96/certifi-2024.2.2.tar.gz", hash = "sha256:0569859f95fc761b18b45ef421b1290a0f65f147e92a1e5eb3e635f9a5e4e66f", size = 164886 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ba/06/a07f096c664aeb9f01624f858c3add0a4e913d6c96257acb4fce61e7de14/certifi-2024.2.2-py3-none-any.whl", hash = "sha256:dc383c07b76109f368f6106eee2b593b04a011ea4d55f652c6ca24a754d1cdd1", size = 163774 },
        ]

        [[distribution]]
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

        [[distribution]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "requests" },
            { name = "requests", extra = "socks", marker = "python_version < '3.10'" },
        ]

        [[distribution]]
        name = "pysocks"
        version = "1.7.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/bd/11/293dd436aea955d45fc4e8a35b6ae7270f5b8e00b53cf6c024c83b657a11/PySocks-1.7.1.tar.gz", hash = "sha256:3f8804571ebe159c380ac6de37643bb4685970655d3bba243530d6558b799aa0", size = 284429 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/8d/59/b4572118e098ac8e46e399a1dd0f2d85403ce8bbaad9ec79373ed6badaf9/PySocks-1.7.1-py3-none-any.whl", hash = "sha256:2725bd0a9925919b9b51739eea5f9e2bae91e83288108a9ad338b2e3a4435ee5", size = 16725 },
        ]

        [[distribution]]
        name = "requests"
        version = "2.31.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/9d/be/10918a2eac4ae9f02f6cfe6414b7a155ccd8f7f9d4380d62fd5b955065c3/requests-2.31.0.tar.gz", hash = "sha256:942c5a758f98d790eaed1a29cb6eefc7ffb0d1cf7af05c3d2791656dbd6ad1e1", size = 110794 }
        dependencies = [
            { name = "certifi" },
            { name = "charset-normalizer" },
            { name = "idna" },
            { name = "urllib3" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/70/8e/0e2d847013cb52cd35b38c009bb167a1a26b2ce6cd6965bf26b47bc0bf44/requests-2.31.0-py3-none-any.whl", hash = "sha256:58cd2187c01e70e6e26505bca751777aa9f2ee0b7f4300988b709f44e013003f", size = 62574 },
        ]

        [distribution.optional-dependencies]
        socks = [
            { name = "pysocks" },
        ]

        [[distribution]]
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

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning.
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

    // Install from the lockfile.
    uv_snapshot!(context_38.filters(), context_38.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning.
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
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 9 packages in [TIME]
    warning: The package `flask==3.0.2` does not have an extra named `foo`.
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [[distribution]]
        name = "blinker"
        version = "1.7.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a1/13/6df5fc090ff4e5d246baf1f45fe9e5623aa8565757dfa5bd243f6a545f9e/blinker-1.7.0.tar.gz", hash = "sha256:e6820ff6fa4e4d1d8e2747c2283749c3f547e4fee112b98555cdcdae32996182", size = 28134 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/fa/2a/7f3714cbc6356a0efec525ce7a0613d581072ed6eb53eb7b9754f33db807/blinker-1.7.0-py3-none-any.whl", hash = "sha256:c3f865d4d54db7abc53758a01601cf343fe55b84c1de4e3fa910e420b438d5b9", size = 13068 },
        ]

        [[distribution]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121 }
        dependencies = [
            { name = "colorama", marker = "platform_system == 'Windows'" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941 },
        ]

        [[distribution]]
        name = "colorama"
        version = "0.4.6"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d8/53/6f443c9a4a8358a93a6792e2acffb9d9d5cb0a5cfd8802644b7b1c9a02e4/colorama-0.4.6.tar.gz", hash = "sha256:08695f5cb7ed6e0531a20572697297273c47b8cae5a63ffc6d6ed5c201be6e44", size = 27697 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d1/d6/3965ed04c63042e047cb6a3e6ed1a63a35087b6a609aa3a15ed8ac56c221/colorama-0.4.6-py2.py3-none-any.whl", hash = "sha256:4f1d9991f5acc0ca119f9d443620b77f9d6b33703e51011c16baf57afb285fc6", size = 25335 },
        ]

        [[distribution]]
        name = "flask"
        version = "3.0.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/3f/e0/a89e8120faea1edbfca1a9b171cff7f2bf62ec860bbafcb2c2387c0317be/flask-3.0.2.tar.gz", hash = "sha256:822c03f4b799204250a7ee84b1eddc40665395333973dfb9deebfe425fefcb7d", size = 675248 }
        dependencies = [
            { name = "blinker" },
            { name = "click" },
            { name = "itsdangerous" },
            { name = "jinja2" },
            { name = "werkzeug" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/93/a6/aa98bfe0eb9b8b15d36cdfd03c8ca86a03968a87f27ce224fb4f766acb23/flask-3.0.2-py3-none-any.whl", hash = "sha256:3232e0e9c850d781933cf0207523d1ece087eb8d87b23777ae38456e2fbe7c6e", size = 101300 },
        ]

        [[distribution]]
        name = "itsdangerous"
        version = "2.1.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/7f/a1/d3fb83e7a61fa0c0d3d08ad0a94ddbeff3731c05212617dff3a94e097f08/itsdangerous-2.1.2.tar.gz", hash = "sha256:5dbbc68b317e5e42f327f9021763545dc3fc3bfe22e6deb96aaf1fc38874156a", size = 56143 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/5f/447e04e828f47465eeab35b5d408b7ebaaaee207f48b7136c5a7267a30ae/itsdangerous-2.1.2-py3-none-any.whl", hash = "sha256:2c2349112351b88699d8d4b6b075022c0808887cb7ad10069318a8b0bc88db44", size = 15749 },
        ]

        [[distribution]]
        name = "jinja2"
        version = "3.1.3"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/b2/5e/3a21abf3cd467d7876045335e681d276ac32492febe6d98ad89562d1a7e1/Jinja2-3.1.3.tar.gz", hash = "sha256:ac8bd6544d4bb2c9792bf3a159e80bba8fda7f07e81bc3aed565432d5925ba90", size = 268261 }
        dependencies = [
            { name = "markupsafe" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/30/6d/6de6be2d02603ab56e72997708809e8a5b0fbfee080735109b40a3564843/Jinja2-3.1.3-py3-none-any.whl", hash = "sha256:7d6d50dd97d52cbc355597bd845fabfbac3f551e1f99619e39a35ce8c370b5fa", size = 133236 },
        ]

        [[distribution]]
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

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "flask" },
        ]

        [[distribution]]
        name = "werkzeug"
        version = "3.0.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/0d/cc/ff1904eb5eb4b455e442834dabf9427331ac0fa02853bf83db817a7dd53d/werkzeug-3.0.1.tar.gz", hash = "sha256:507e811ecea72b18a404947aded4b3390e1db8f826b494d76550ef45bb3b1dcc", size = 801436 }
        dependencies = [
            { name = "markupsafe" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c3/fc/254c3e9b5feb89ff5b9076a23218dafbc99c96ac5941e900b71206e6313b/werkzeug-3.0.1-py3-none-any.whl", hash = "sha256:90a285dc0e42ad56b34e696398b8122ee4c681833fb35b8334a095d82c56da10", size = 226669 },
        ]
        "###
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning.
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
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [[distribution]]
        name = "iniconfig"
        version = "1.1.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/23/a2/97899f6bd0e873fed3a7e67ae8d3a08b21799430fb4da15cfedf10d6e2c2/iniconfig-1.1.1.tar.gz", hash = "sha256:bc3af051d7d14b2ee5ef9969666def0cd1a000e121eaea580d4a313df4b37f32", size = 8104 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl", hash = "sha256:011e24c64b7f47f6ebd835bb12a743f2fbe9a26d4cecaa7f53bc4f35ee9da8b3", size = 4990 },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]
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
        "#,
    )?;

    // Ensure that the locked version is still respected.
    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [[distribution]]
        name = "iniconfig"
        version = "1.1.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/23/a2/97899f6bd0e873fed3a7e67ae8d3a08b21799430fb4da15cfedf10d6e2c2/iniconfig-1.1.1.tar.gz", hash = "sha256:bc3af051d7d14b2ee5ef9969666def0cd1a000e121eaea580d4a313df4b37f32", size = 8104 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl", hash = "sha256:011e24c64b7f47f6ebd835bb12a743f2fbe9a26d4cecaa7f53bc4f35ee9da8b3", size = 4990 },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]
        "###
        );
    });

    // Run with `--upgrade`; ensure that `iniconfig` is upgraded.
    uv_snapshot!(context.filters(), context.lock().arg("--upgrade"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [[distribution]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]
        "###
        );
    });

    Ok(())
}

/// Respect locked versions with `uv lock`, unless `--upgrade` is passed.
#[test]
fn lock_git_sha() -> Result<()> {
    let context = TestContext::new("3.12");

    // Lock to a specific commit on `main`.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@0dacfd662c64cb4ceb16e6cf65a157a8b715b979"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "uv-public-pypackage" },
        ]

        [[distribution]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = { git = "https://github.com/astral-test/uv-public-pypackage?rev=0dacfd662c64cb4ceb16e6cf65a157a8b715b979#0dacfd662c64cb4ceb16e6cf65a157a8b715b979" }
        "###
        );
    });

    // Rewrite the lockfile, as if it were locked against `main`.
    let lock = lock.replace("rev=0dacfd662c64cb4ceb16e6cf65a157a8b715b979", "rev=main");
    fs_err::write(context.temp_dir.join("uv.lock"), lock)?;

    // Lock `anyio` against `main`.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@main"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    // The lockfile should resolve to `0dacfd662c64cb4ceb16e6cf65a157a8b715b979`, even though it's
    // not the latest commit on `main`.
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "uv-public-pypackage" },
        ]

        [[distribution]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = { git = "https://github.com/astral-test/uv-public-pypackage?rev=main#0dacfd662c64cb4ceb16e6cf65a157a8b715b979" }
        "###
        );
    });

    // Relock with `--upgrade`.
    uv_snapshot!(context.filters(), context.lock().arg("--upgrade-package").arg("uv-public-pypackage"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    // The lockfile should resolve to `b270df1a2fb5d012294e9aaf05e7e0bab1e6a389`, the latest commit
    // on `main`.
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "uv-public-pypackage" },
        ]

        [[distribution]]
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
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
      × No solution found when resolving dependencies:
      ╰─▶ Because the requested Python version (>=3.7) does not satisfy Python>=3.8 and the requested Python version (>=3.7) does not satisfy Python>=3.7.9,<3.8, we can conclude that Python>=3.7.9 is incompatible.
          And because pygls>=1.1.0,<=1.2.1 depends on Python>=3.7.9,<4 and only pygls<=1.3.0 is available, we can conclude that any of:
              pygls>=1.1.0,<1.3.0
              pygls>1.3.0
           cannot be used. (1)

          Because the requested Python version (>=3.7) does not satisfy Python>=3.8 and pygls==1.3.0 depends on Python>=3.8, we can conclude that pygls==1.3.0 cannot be used.
          And because we know from (1) that any of:
              pygls>=1.1.0,<1.3.0
              pygls>1.3.0
           cannot be used, we can conclude that pygls>=1.1.0 cannot be used.
          And because project==0.1.0 depends on pygls>=1.1.0, we can conclude that project==0.1.0 cannot be used.
          And because only project==0.1.0 is available and you require project, we can conclude that the requirements are unsatisfiable.

          hint: The `Requires-Python` requirement (>=3.7) includes Python versions that are not supported by your dependencies (e.g., pygls>=1.1.0,<=1.2.1 only supports >=3.7.9, <4). Consider using a more restrictive `Requires-Python` requirement (like >=3.7.9, <4).
    "###);

    // Require >=3.7, and allow locking to a version of `pygls` that is compatible (==1.0.1).
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.7"
        dependencies = ["pygls"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 10 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(&lockfile)?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.7"

        [[distribution]]
        name = "attrs"
        version = "23.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/e3/fc/f800d51204003fa8ae392c4e8278f256206e7a919b708eef054f5f4b650d/attrs-23.2.0.tar.gz", hash = "sha256:935dc3b529c262f6cf76e50877d35a4bd3c1de194fd41f47a2b7ae8f19971f30", size = 780820 }
        dependencies = [
            { name = "importlib-metadata", marker = "python_version < '3.8'" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e0/44/827b2a91a5816512fcaf3cc4ebc465ccd5d598c45cefa6703fcf4a79018f/attrs-23.2.0-py3-none-any.whl", hash = "sha256:99b87a485a5820b23b879f04c2305b44b951b502fd64be915879d77a7e8fc6f1", size = 60752 },
        ]

        [[distribution]]
        name = "cattrs"
        version = "23.1.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/68/d4/27f9fd840e74d51b6d6a024d39ff495b56ffde71d28eb82758b7b85d0617/cattrs-23.1.2.tar.gz", hash = "sha256:db1c821b8c537382b2c7c66678c3790091ca0275ac486c76f3c8f3920e83c657", size = 39998 }
        dependencies = [
            { name = "attrs" },
            { name = "exceptiongroup", marker = "python_version < '3.11'" },
            { name = "typing-extensions", marker = "python_version < '3.11'" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/3a/ba/05df14efaa0624fac6b1510e87f5ce446208d2f6ce50270a89b6268aebfe/cattrs-23.1.2-py3-none-any.whl", hash = "sha256:b2bb14311ac17bed0d58785e5a60f022e5431aca3932e3fc5cc8ed8639de50a4", size = 50845 },
        ]

        [[distribution]]
        name = "exceptiongroup"
        version = "1.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/8e/1c/beef724eaf5b01bb44b6338c8c3494eff7cab376fab4904cfbbc3585dc79/exceptiongroup-1.2.0.tar.gz", hash = "sha256:91f5c769735f051a4290d52edd0858999b57e5876e9f85937691bd4c9fa3ed68", size = 26264 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b8/9a/5028fd52db10e600f1c4674441b968cf2ea4959085bfb5b99fb1250e5f68/exceptiongroup-1.2.0-py3-none-any.whl", hash = "sha256:4bfd3996ac73b41e9b9628b04e079f193850720ea5945fc96a08633c66912f14", size = 16210 },
        ]

        [[distribution]]
        name = "importlib-metadata"
        version = "6.7.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a3/82/f6e29c8d5c098b6be61460371c2c5591f4a335923639edec43b3830650a4/importlib_metadata-6.7.0.tar.gz", hash = "sha256:1aaf550d4f73e5d6783e7acb77aec43d49da8017410afae93822cc9cca98c4d4", size = 53569 }
        dependencies = [
            { name = "typing-extensions", marker = "python_version < '3.8'" },
            { name = "zipp" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ff/94/64287b38c7de4c90683630338cf28f129decbba0a44f0c6db35a873c73c4/importlib_metadata-6.7.0-py3-none-any.whl", hash = "sha256:cb52082e659e97afc5dac71e79de97d8681de3aa07ff18578330904a9d18e5b5", size = 22934 },
        ]

        [[distribution]]
        name = "lsprotocol"
        version = "2023.0.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/9d/f6/6e80484ec078d0b50699ceb1833597b792a6c695f90c645fbaf54b947e6f/lsprotocol-2023.0.1.tar.gz", hash = "sha256:cc5c15130d2403c18b734304339e51242d3018a05c4f7d0f198ad6e0cd21861d", size = 69434 }
        dependencies = [
            { name = "attrs" },
            { name = "cattrs" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/8d/37/2351e48cb3309673492d3a8c59d407b75fb6630e560eb27ecd4da03adc9a/lsprotocol-2023.0.1-py3-none-any.whl", hash = "sha256:c75223c9e4af2f24272b14c6375787438279369236cd568f596d4951052a60f2", size = 70826 },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "pygls" },
        ]

        [[distribution]]
        name = "pygls"
        version = "1.0.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/8e/27/58ff0f76b379fc11a1d03e8d4b4e96fd0abb463d27709a7fb4193bcdbbc4/pygls-1.0.1.tar.gz", hash = "sha256:f3ee98ddbb4690eb5c755bc32ba7e129607f14cbd313575f33d0cea443b78cb2", size = 674546 }
        dependencies = [
            { name = "lsprotocol" },
            { name = "typeguard" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/da/9b/4fd77a060068f2f3f46f97ed6ba8762c5a73f11ef0c196cfd34f3a9be878/pygls-1.0.1-py3-none-any.whl", hash = "sha256:adacc96da77598c70f46acfdfd1481d3da90cd54f639f7eee52eb6e4dbd57b55", size = 40367 },
        ]

        [[distribution]]
        name = "typeguard"
        version = "2.13.3"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/3a/38/c61bfcf62a7b572b5e9363a802ff92559cb427ee963048e1442e3aef7490/typeguard-2.13.3.tar.gz", hash = "sha256:00edaa8da3a133674796cf5ea87d9f4b4c367d77476e185e80251cc13dfbb8c4", size = 40604 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/9a/bb/d43e5c75054e53efce310e79d63df0ac3f25e34c926be5dffb7d283fb2a8/typeguard-2.13.3-py3-none-any.whl", hash = "sha256:5e3e3be01e887e7eafae5af63d1f36c849aaa94e3a0112097312aabfa16284f1", size = 17605 },
        ]

        [[distribution]]
        name = "typing-extensions"
        version = "4.7.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/3c/8b/0111dd7d6c1478bf83baa1cab85c686426c7a6274119aceb2bd9d35395ad/typing_extensions-4.7.1.tar.gz", hash = "sha256:b75ddc264f0ba5615db7ba217daeb99701ad295353c45f9e95963337ceeeffb2", size = 72876 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ec/6b/63cc3df74987c36fe26157ee12e09e8f9db4de771e0f3404263117e75b95/typing_extensions-4.7.1-py3-none-any.whl", hash = "sha256:440d5dd3af93b060174bf433bccd69b0babc3b15b1a8dca43789fd7f61514b36", size = 33232 },
        ]

        [[distribution]]
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
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 9 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(&lockfile)?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.7.9"

        [[distribution]]
        name = "attrs"
        version = "23.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/e3/fc/f800d51204003fa8ae392c4e8278f256206e7a919b708eef054f5f4b650d/attrs-23.2.0.tar.gz", hash = "sha256:935dc3b529c262f6cf76e50877d35a4bd3c1de194fd41f47a2b7ae8f19971f30", size = 780820 }
        dependencies = [
            { name = "importlib-metadata", marker = "python_version < '3.8'" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e0/44/827b2a91a5816512fcaf3cc4ebc465ccd5d598c45cefa6703fcf4a79018f/attrs-23.2.0-py3-none-any.whl", hash = "sha256:99b87a485a5820b23b879f04c2305b44b951b502fd64be915879d77a7e8fc6f1", size = 60752 },
        ]

        [[distribution]]
        name = "cattrs"
        version = "23.1.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/68/d4/27f9fd840e74d51b6d6a024d39ff495b56ffde71d28eb82758b7b85d0617/cattrs-23.1.2.tar.gz", hash = "sha256:db1c821b8c537382b2c7c66678c3790091ca0275ac486c76f3c8f3920e83c657", size = 39998 }
        dependencies = [
            { name = "attrs" },
            { name = "exceptiongroup", marker = "python_version < '3.11'" },
            { name = "typing-extensions", marker = "python_version < '3.11'" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/3a/ba/05df14efaa0624fac6b1510e87f5ce446208d2f6ce50270a89b6268aebfe/cattrs-23.1.2-py3-none-any.whl", hash = "sha256:b2bb14311ac17bed0d58785e5a60f022e5431aca3932e3fc5cc8ed8639de50a4", size = 50845 },
        ]

        [[distribution]]
        name = "exceptiongroup"
        version = "1.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/8e/1c/beef724eaf5b01bb44b6338c8c3494eff7cab376fab4904cfbbc3585dc79/exceptiongroup-1.2.0.tar.gz", hash = "sha256:91f5c769735f051a4290d52edd0858999b57e5876e9f85937691bd4c9fa3ed68", size = 26264 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b8/9a/5028fd52db10e600f1c4674441b968cf2ea4959085bfb5b99fb1250e5f68/exceptiongroup-1.2.0-py3-none-any.whl", hash = "sha256:4bfd3996ac73b41e9b9628b04e079f193850720ea5945fc96a08633c66912f14", size = 16210 },
        ]

        [[distribution]]
        name = "importlib-metadata"
        version = "6.7.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a3/82/f6e29c8d5c098b6be61460371c2c5591f4a335923639edec43b3830650a4/importlib_metadata-6.7.0.tar.gz", hash = "sha256:1aaf550d4f73e5d6783e7acb77aec43d49da8017410afae93822cc9cca98c4d4", size = 53569 }
        dependencies = [
            { name = "typing-extensions", marker = "python_version < '3.8'" },
            { name = "zipp" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ff/94/64287b38c7de4c90683630338cf28f129decbba0a44f0c6db35a873c73c4/importlib_metadata-6.7.0-py3-none-any.whl", hash = "sha256:cb52082e659e97afc5dac71e79de97d8681de3aa07ff18578330904a9d18e5b5", size = 22934 },
        ]

        [[distribution]]
        name = "lsprotocol"
        version = "2023.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/3e/fe/f7671a4fb28606ff1663bba60aff6af21b1e43a977c74c33db13cb83680f/lsprotocol-2023.0.0.tar.gz", hash = "sha256:c9d92e12a3f4ed9317d3068226592860aab5357d93cf5b2451dc244eee8f35f2", size = 69399 }
        dependencies = [
            { name = "attrs" },
            { name = "cattrs" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2d/5b/f18eb1823a4cee9bed70cdcc25eed5a75845367c42e63a79010a7c34f8a7/lsprotocol-2023.0.0-py3-none-any.whl", hash = "sha256:e85fc87ee26c816adca9eb497bb3db1a7c79c477a11563626e712eaccf926a05", size = 70789 },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "pygls" },
        ]

        [[distribution]]
        name = "pygls"
        version = "1.2.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/e6/94/534c11ba5475df09542e48d751a66e0448d52bbbb92cbef5541deef7760d/pygls-1.2.1.tar.gz", hash = "sha256:04f9b9c115b622dcc346fb390289066565343d60245a424eca77cb429b911ed8", size = 45274 }
        dependencies = [
            { name = "lsprotocol" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/36/31/3799444d3f072ffca1a35eb02a48f964384cc13f001125e87d9f0748687b/pygls-1.2.1-py3-none-any.whl", hash = "sha256:7dcfcf12b6f15beb606afa46de2ed348b65a279c340ef2242a9a35c22eeafe94", size = 55983 },
        ]

        [[distribution]]
        name = "typing-extensions"
        version = "4.7.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/3c/8b/0111dd7d6c1478bf83baa1cab85c686426c7a6274119aceb2bd9d35395ad/typing_extensions-4.7.1.tar.gz", hash = "sha256:b75ddc264f0ba5615db7ba217daeb99701ad295353c45f9e95963337ceeeffb2", size = 72876 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ec/6b/63cc3df74987c36fe26157ee12e09e8f9db4de771e0f3404263117e75b95/typing_extensions-4.7.1-py3-none-any.whl", hash = "sha256:440d5dd3af93b060174bf433bccd69b0babc3b15b1a8dca43789fd7f61514b36", size = 33232 },
        ]

        [[distribution]]
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
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 5 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(&lockfile)?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [[distribution]]
        name = "attrs"
        version = "23.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/e3/fc/f800d51204003fa8ae392c4e8278f256206e7a919b708eef054f5f4b650d/attrs-23.2.0.tar.gz", hash = "sha256:935dc3b529c262f6cf76e50877d35a4bd3c1de194fd41f47a2b7ae8f19971f30", size = 780820 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e0/44/827b2a91a5816512fcaf3cc4ebc465ccd5d598c45cefa6703fcf4a79018f/attrs-23.2.0-py3-none-any.whl", hash = "sha256:99b87a485a5820b23b879f04c2305b44b951b502fd64be915879d77a7e8fc6f1", size = 60752 },
        ]

        [[distribution]]
        name = "cattrs"
        version = "23.2.3"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/1e/57/c6ccd22658c4bcb3beb3f1c262e1f170cf136e913b122763d0ddd328d284/cattrs-23.2.3.tar.gz", hash = "sha256:a934090d95abaa9e911dac357e3a8699e0b4b14f8529bcc7d2b1ad9d51672b9f", size = 610215 }
        dependencies = [
            { name = "attrs" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b3/0d/cd4a4071c7f38385dc5ba91286723b4d1090b87815db48216212c6c6c30e/cattrs-23.2.3-py3-none-any.whl", hash = "sha256:0341994d94971052e9ee70662542699a3162ea1e0c62f7ce1b4a57f563685108", size = 57474 },
        ]

        [[distribution]]
        name = "lsprotocol"
        version = "2023.0.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/9d/f6/6e80484ec078d0b50699ceb1833597b792a6c695f90c645fbaf54b947e6f/lsprotocol-2023.0.1.tar.gz", hash = "sha256:cc5c15130d2403c18b734304339e51242d3018a05c4f7d0f198ad6e0cd21861d", size = 69434 }
        dependencies = [
            { name = "attrs" },
            { name = "cattrs" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/8d/37/2351e48cb3309673492d3a8c59d407b75fb6630e560eb27ecd4da03adc9a/lsprotocol-2023.0.1-py3-none-any.whl", hash = "sha256:c75223c9e4af2f24272b14c6375787438279369236cd568f596d4951052a60f2", size = 70826 },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "pygls" },
        ]

        [[distribution]]
        name = "pygls"
        version = "1.3.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/e9/8d/31b50ac0879464049d744a1ddf00dc6474433eb55d40fa0c8e8510591ad2/pygls-1.3.0.tar.gz", hash = "sha256:1b44ace89c9382437a717534f490eadc6fda7c0c6c16ac1eaaf5568e345e4fb8", size = 45539 }
        dependencies = [
            { name = "cattrs" },
            { name = "lsprotocol" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/4e/1e/643070d8f5c851958662e7e5df16d9c3a068a598a7ee7bb2eb8d95b4e5d7/pygls-1.3.0-py3-none-any.whl", hash = "sha256:d4a01414b6ed4e34e7e8fd29b77d3e88c29615df7d0bbff49bf019e15ec04b8f", size = 56031 },
        ]
        "###
        );
    });

    // Validate that attempting to install with an unsupported Python version raises an error.
    let context38 = TestContext::new("3.8");

    fs_err::copy(pyproject_toml, context38.temp_dir.join("pyproject.toml"))?;
    fs_err::copy(&lockfile, context38.temp_dir.join("uv.lock"))?;

    let filters: Vec<_> = context38
        .filters()
        .into_iter()
        .chain(context.filters())
        // Platform independent message for the missing Python installation
        .chain([(" or `py` launcher", "")])
        .collect();

    // Install from the lockfile.
    // Note we need to disable Python fetches or we'll just download 3.12
    uv_snapshot!(filters, context38.sync().arg("--python-fetch").arg("manual"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning.
    error: No interpreter found for Python >=3.12 in system path
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
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 6 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(lockfile)?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.11, <3.12"

        [[distribution]]
        name = "attrs"
        version = "23.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/e3/fc/f800d51204003fa8ae392c4e8278f256206e7a919b708eef054f5f4b650d/attrs-23.2.0.tar.gz", hash = "sha256:935dc3b529c262f6cf76e50877d35a4bd3c1de194fd41f47a2b7ae8f19971f30", size = 780820 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e0/44/827b2a91a5816512fcaf3cc4ebc465ccd5d598c45cefa6703fcf4a79018f/attrs-23.2.0-py3-none-any.whl", hash = "sha256:99b87a485a5820b23b879f04c2305b44b951b502fd64be915879d77a7e8fc6f1", size = 60752 },
        ]

        [[distribution]]
        name = "cattrs"
        version = "23.2.3"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/1e/57/c6ccd22658c4bcb3beb3f1c262e1f170cf136e913b122763d0ddd328d284/cattrs-23.2.3.tar.gz", hash = "sha256:a934090d95abaa9e911dac357e3a8699e0b4b14f8529bcc7d2b1ad9d51672b9f", size = 610215 }
        dependencies = [
            { name = "attrs" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b3/0d/cd4a4071c7f38385dc5ba91286723b4d1090b87815db48216212c6c6c30e/cattrs-23.2.3-py3-none-any.whl", hash = "sha256:0341994d94971052e9ee70662542699a3162ea1e0c62f7ce1b4a57f563685108", size = 57474 },
        ]

        [[distribution]]
        name = "linehaul"
        version = "1.0.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/f8/e7/74d1bd36ed26ac43bfe22e97129edaa7066f7af4bf76084b9493cd581d58/linehaul-1.0.1.tar.gz", hash = "sha256:09d71b1f6a9ab92dd8c763b3d099e4ae05c2845ee783a02d5fe731e6e2a6a997", size = 19410 }
        dependencies = [
            { name = "cattrs" },
            { name = "packaging" },
            { name = "pyparsing" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/03/73/c73588052198be06462d1a7c4653b602a109a0df0208c59e58075dc3bc73/linehaul-1.0.1-py3-none-any.whl", hash = "sha256:d19ca669008dad910868dfae7f904dfc5362583729bda344799cf7ea2ad5ef12", size = 27848 },
        ]

        [[distribution]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488 },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "linehaul" },
        ]

        [[distribution]]
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
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 6 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(lockfile)?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.11"

        [[distribution]]
        name = "attrs"
        version = "23.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/e3/fc/f800d51204003fa8ae392c4e8278f256206e7a919b708eef054f5f4b650d/attrs-23.2.0.tar.gz", hash = "sha256:935dc3b529c262f6cf76e50877d35a4bd3c1de194fd41f47a2b7ae8f19971f30", size = 780820 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e0/44/827b2a91a5816512fcaf3cc4ebc465ccd5d598c45cefa6703fcf4a79018f/attrs-23.2.0-py3-none-any.whl", hash = "sha256:99b87a485a5820b23b879f04c2305b44b951b502fd64be915879d77a7e8fc6f1", size = 60752 },
        ]

        [[distribution]]
        name = "cattrs"
        version = "23.2.3"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/1e/57/c6ccd22658c4bcb3beb3f1c262e1f170cf136e913b122763d0ddd328d284/cattrs-23.2.3.tar.gz", hash = "sha256:a934090d95abaa9e911dac357e3a8699e0b4b14f8529bcc7d2b1ad9d51672b9f", size = 610215 }
        dependencies = [
            { name = "attrs" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b3/0d/cd4a4071c7f38385dc5ba91286723b4d1090b87815db48216212c6c6c30e/cattrs-23.2.3-py3-none-any.whl", hash = "sha256:0341994d94971052e9ee70662542699a3162ea1e0c62f7ce1b4a57f563685108", size = 57474 },
        ]

        [[distribution]]
        name = "linehaul"
        version = "1.0.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/f8/e7/74d1bd36ed26ac43bfe22e97129edaa7066f7af4bf76084b9493cd581d58/linehaul-1.0.1.tar.gz", hash = "sha256:09d71b1f6a9ab92dd8c763b3d099e4ae05c2845ee783a02d5fe731e6e2a6a997", size = 19410 }
        dependencies = [
            { name = "cattrs" },
            { name = "packaging" },
            { name = "pyparsing" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/03/73/c73588052198be06462d1a7c4653b602a109a0df0208c59e58075dc3bc73/linehaul-1.0.1-py3-none-any.whl", hash = "sha256:d19ca669008dad910868dfae7f904dfc5362583729bda344799cf7ea2ad5ef12", size = 27848 },
        ]

        [[distribution]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488 },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "linehaul" },
        ]

        [[distribution]]
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
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    warning: The workspace `requires-python` field does not contain a lower bound: `<=3.12`. Set a lower bound to indicate the minimum compatible Python version (e.g., `>=3.11`).
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(lockfile)?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = "<=3.12"

        [[distribution]]
        name = "iniconfig"
        version = "1.1.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/23/a2/97899f6bd0e873fed3a7e67ae8d3a08b21799430fb4da15cfedf10d6e2c2/iniconfig-1.1.1.tar.gz", hash = "sha256:bc3af051d7d14b2ee5ef9969666def0cd1a000e121eaea580d4a313df4b37f32", size = 8104 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl", hash = "sha256:011e24c64b7f47f6ebd835bb12a743f2fbe9a26d4cecaa7f53bc4f35ee9da8b3", size = 4990 },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]
        "###
        );
    });

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
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 4 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(lockfile)?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.8"

        [[distribution]]
        name = "attrs"
        version = "23.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/e3/fc/f800d51204003fa8ae392c4e8278f256206e7a919b708eef054f5f4b650d/attrs-23.2.0.tar.gz", hash = "sha256:935dc3b529c262f6cf76e50877d35a4bd3c1de194fd41f47a2b7ae8f19971f30", size = 780820 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e0/44/827b2a91a5816512fcaf3cc4ebc465ccd5d598c45cefa6703fcf4a79018f/attrs-23.2.0-py3-none-any.whl", hash = "sha256:99b87a485a5820b23b879f04c2305b44b951b502fd64be915879d77a7e8fc6f1", size = 60752 },
        ]

        [[distribution]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "attrs" },
            { name = "iniconfig" },
            { name = "typing-extensions", marker = "python_full_version <= '3.10' or python_full_version > '3.10'" },
        ]

        [[distribution]]
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
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 3 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [[distribution]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]

        [distribution.dev-dependencies]
        dev = [
            { name = "typing-extensions" },
        ]

        [[distribution]]
        name = "typing-extensions"
        version = "4.12.2"
        source = { url = "https://files.pythonhosted.org/packages/26/9f/ad63fc0248c5379346306f8668cda6e2e2e9c95e01216d2b8ffd9ff037d0/typing_extensions-4.12.2-py3-none-any.whl" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/26/9f/ad63fc0248c5379346306f8668cda6e2e2e9c95e01216d2b8ffd9ff037d0/typing_extensions-4.12.2-py3-none-any.whl", hash = "sha256:04e5ca0351e0f3f85c6853954072df659d0d13fac324d0072316b67d7794700d" },
        ]
        "###
        );
    });

    // Install from the lockfile, excluding development dependencies.
    uv_snapshot!(context.filters(), context.sync().arg("--no-dev"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning.
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     + project==0.1.0 (from file://[TEMP_DIR]/)
    "###);

    // Install from the lockfile, including development dependencies (the default).
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning.
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
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [[distribution]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig" },
        ]
        "###
        );
    });

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
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 2 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [[distribution]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "iniconfig", marker = "implementation_name == 'cpython'" },
        ]
        "###
        );
    });

    Ok(())
}

/// Check relative and absolute path handling in lockfiles.
#[test]
fn relative_and_absolute_paths() -> Result<()> {
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
        requires = ["hatchling"]
        build-backend = "hatchling.build"
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
        requires = ["hatchling"]
        build-backend = "hatchling.build"
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
        requires = ["hatchling"]
        build-backend = "hatchling.build"
    "#})?;
    context.temp_dir.child("c/c/__init__.py").touch()?;

    uv_snapshot!(context.filters(), context.lock().arg("--preview"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.11, <3.13"

        [[distribution]]
        name = "a"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "b" },
            { name = "c" },
        ]

        [[distribution]]
        name = "b"
        version = "0.1.0"
        source = { directory = "b" }

        [[distribution]]
        name = "c"
        version = "0.1.0"
        source = { directory = "[TEMP_DIR]/c" }
        "###
        );
    });

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
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 11 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"

        [[distribution]]
        name = "argparse"
        version = "1.4.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/18/dd/e617cfc3f6210ae183374cd9f6a26b20514bbb5a792af97949c5aacddf0f/argparse-1.4.0.tar.gz", hash = "sha256:62b089a55be1d8949cd2bc7e0df0bddb9e028faefc8c32038cc84862aefdd6e4", size = 70508 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f2/94/3af39d34be01a24a6e65433d19e107099374224905f1e0cc6bbe1fd22a2f/argparse-1.4.0-py2.py3-none-any.whl", hash = "sha256:c31647edb69fd3d465a847ea3157d37bed1f95f19760b11a47aa91c04b666314", size = 23000 },
        ]

        [[distribution]]
        name = "extras"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/be/18/0b7283f0ebf6ad4bb6b9937538495eadf05ef097b102946b9445c4242636/extras-1.0.0.tar.gz", hash = "sha256:132e36de10b9c91d5d4cc620160a476e0468a88f16c9431817a6729611a81b4e", size = 6759 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/03/e9/e915af1f97914cd0bc021e125fd1bfd4106de614a275e4b6866dd9a209ac/extras-1.0.0-py2.py3-none-any.whl", hash = "sha256:f689f08df47e2decf76aa6208c081306e7bd472630eb1ec8a875c67de2366e87", size = 7279 },
        ]

        [[distribution]]
        name = "fixtures"
        version = "3.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/84/be/94ecbc3f2487bd14aa8b44852f498086219b7cc0c8250ee65a03e2c63119/fixtures-3.0.0.tar.gz", hash = "sha256:fcf0d60234f1544da717a9738325812de1f42c2fa085e2d9252d8fff5712b2ef", size = 56629 }
        dependencies = [
            { name = "pbr" },
            { name = "six" },
            { name = "testtools" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a8/28/7eed6bf76792f418029a18d5b2ace87ce7562927cdd00f1cefe481cd148f/fixtures-3.0.0-py2.py3-none-any.whl", hash = "sha256:2a551b0421101de112d9497fb5f6fd25e5019391c0fbec9bad591ecae981420d", size = 67478 },
        ]

        [[distribution]]
        name = "linecache2"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/44/b0/963c352372c242f9e40db02bbc6a39ae51bde15dddee8523fe4aca94a97e/linecache2-1.0.0.tar.gz", hash = "sha256:4b26ff4e7110db76eeb6f5a7b64a82623839d595c2038eeda662f2a2db78e97c", size = 11013 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c7/a3/c5da2a44c85bfbb6eebcfc1dde24933f8704441b98fdde6528f4831757a6/linecache2-1.0.0-py2.py3-none-any.whl", hash = "sha256:e78be9c0a0dfcbac712fe04fbf92b96cddae80b1b842f24248214c8496f006ef", size = 12967 },
        ]

        [[distribution]]
        name = "pbr"
        version = "6.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/8d/c2/ee43b3b11bf2b40e56536183fc9f22afbb04e882720332b6276ee2454c24/pbr-6.0.0.tar.gz", hash = "sha256:d1377122a5a00e2f940ee482999518efe16d745d423a670c27773dfbc3c9a7d9", size = 123150 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/64/dd/171c9fb653591cf265bcc89c436eec75c9bde3dec921cc236fa71e5698df/pbr-6.0.0-py2.py3-none-any.whl", hash = "sha256:4a7317d5e3b17a3dccb6a8cfe67dab65b20551404c52c8ed41279fa4f0cb4cda", size = 107506 },
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "fixtures" },
            { name = "testtools" },
        ]

        [[distribution]]
        name = "python-mimeparse"
        version = "1.6.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/0f/40/ac5f9e44a55b678c3cd881b4c3376e5b002677dbeab6fb3a50bac5d50d29/python-mimeparse-1.6.0.tar.gz", hash = "sha256:76e4b03d700a641fd7761d3cd4fdbbdcd787eade1ebfac43f877016328334f78", size = 6541 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/26/2e/03bce213a9bf02a2750dcb04e761785e9c763fc11071edc4b447eacbb842/python_mimeparse-1.6.0-py2.py3-none-any.whl", hash = "sha256:a295f03ff20341491bfe4717a39cd0a8cc9afad619ba44b77e86b0ab8a2b8282", size = 6057 },
        ]

        [[distribution]]
        name = "six"
        version = "1.16.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/71/39/171f1c67cd00715f190ba0b100d606d440a28c93c7714febeca8b79af85e/six-1.16.0.tar.gz", hash = "sha256:1e61c37477a1626458e36f7b1d82aa5c9b094fa4802892072e49de9c60c4c926", size = 34041 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d9/5a/e7c31adbe875f2abbb91bd84cf2dc52d792b5a01506781dbcf25c91daf11/six-1.16.0-py2.py3-none-any.whl", hash = "sha256:8abb2f1d86890a2dfb989f9a77cfcfd3e47c2a354b01111771326f8aa26e0254", size = 11053 },
        ]

        [[distribution]]
        name = "testtools"
        version = "2.3.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/e5/d4/9b22df94d0d5c83affe2517295c85fa2d9917f3cafa7dc7f6b1ce4135b00/testtools-2.3.0.tar.gz", hash = "sha256:5827ec6cf8233e0f29f51025addd713ca010061204fdea77484a2934690a0559", size = 231559 }
        dependencies = [
            { name = "extras" },
            { name = "fixtures" },
            { name = "pbr" },
            { name = "python-mimeparse" },
            { name = "six" },
            { name = "traceback2" },
            { name = "unittest2" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/87/74/a4d55da28d7bba6d6f49430f22a62afd8472cb24a63fa61daef80d3e821b/testtools-2.3.0-py2.py3-none-any.whl", hash = "sha256:a2be448869171b6e0f26d9544088b8b98439ec180ce272040236d570a40bcbed", size = 184636 },
        ]

        [[distribution]]
        name = "traceback2"
        version = "1.4.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/eb/7f/e20ba11390bdfc55117c8c6070838ec914e6f0053a602390a598057884eb/traceback2-1.4.0.tar.gz", hash = "sha256:05acc67a09980c2ecfedd3423f7ae0104839eccb55fc645773e1caa0951c3030", size = 15872 }
        dependencies = [
            { name = "linecache2" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/17/0a/6ac05a3723017a967193456a2efa0aa9ac4b51456891af1e2353bb9de21e/traceback2-1.4.0-py2.py3-none-any.whl", hash = "sha256:8253cebec4b19094d67cc5ed5af99bf1dba1285292226e98a31929f87a5d6b23", size = 16793 },
        ]

        [[distribution]]
        name = "unittest2"
        version = "1.1.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/7f/c4/2b0e2d185d9d60772c10350d9853646832609d2f299a8300ab730f199db4/unittest2-1.1.0.tar.gz", hash = "sha256:22882a0e418c284e1f718a822b3b022944d53d2d908e1690b319a9d3eb2c0579", size = 81432 }
        dependencies = [
            { name = "argparse" },
            { name = "six" },
            { name = "traceback2" },
        ]
        wheels = [
            { url = "https://files.pythonhosted.org/packages/72/20/7f0f433060a962200b7272b8c12ba90ef5b903e218174301d0abfd523813/unittest2-1.1.0-py2.py3-none-any.whl", hash = "sha256:13f77d0875db6d9b435e1d4f41e74ad4cc2eb6e1d5c824996092b3430f088bb8", size = 96379 },
        ]
        "###
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning.
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

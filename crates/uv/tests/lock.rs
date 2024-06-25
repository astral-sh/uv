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
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/c6/b3/fefbf7e78ab3b805dec67d698dc18dd505af7a18a8dd08868c9b4fa736b5/anyio-3.7.0.tar.gz", hash = "sha256:275d9973793619a5374e1c89a4f4ad3f4b0a5510a2b5b939444bee8f4c4d37ce", size = 142737 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/68/fe/7ce1926952c8a403b35029e194555558514b365ad77d75125f521a2bec62/anyio-3.7.0-py3-none-any.whl", hash = "sha256:eddca883c4175f14df8aedce21054bfca3adb70ffe76a9f607aef9d7fa2ea7f0", size = 80873 }]

        [[distribution.dependencies]]
        name = "idna"

        [[distribution.dependencies]]
        name = "sniffio"

        [[distribution]]
        name = "idna"
        version = "3.6"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "anyio"

        [[distribution]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 }]
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
        source = "editable+."

        [[distribution.dependencies]]
        name = "source-distribution"

        [[distribution]]
        name = "source-distribution"
        version = "0.0.1"
        source = "registry+https://pypi.org/simple"
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
        source = "editable+."

        [[distribution.dependencies]]
        name = "uv-public-pypackage"

        [[distribution]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = "git+https://github.com/astral-test/uv-public-pypackage?rev=0.0.1#0dacfd662c64cb4ceb16e6cf65a157a8b715b979"
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
        source = "direct+https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl"
        wheels = [{ url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8" }]

        [[distribution.dependencies]]
        name = "idna"

        [[distribution.dependencies]]
        name = "sniffio"

        [[distribution]]
        name = "idna"
        version = "3.6"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "anyio"

        [[distribution]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 }]
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
        source = "direct+https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz"
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6" }

        [[distribution.dependencies]]
        name = "idna"

        [[distribution.dependencies]]
        name = "sniffio"

        [[distribution]]
        name = "idna"
        version = "3.6"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "anyio"

        [[distribution]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 }]
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
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/c6/b3/fefbf7e78ab3b805dec67d698dc18dd505af7a18a8dd08868c9b4fa736b5/anyio-3.7.0.tar.gz", hash = "sha256:275d9973793619a5374e1c89a4f4ad3f4b0a5510a2b5b939444bee8f4c4d37ce", size = 142737 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/68/fe/7ce1926952c8a403b35029e194555558514b365ad77d75125f521a2bec62/anyio-3.7.0-py3-none-any.whl", hash = "sha256:eddca883c4175f14df8aedce21054bfca3adb70ffe76a9f607aef9d7fa2ea7f0", size = 80873 }]

        [[distribution.dependencies]]
        name = "idna"

        [[distribution.dependencies]]
        name = "sniffio"

        [[distribution]]
        name = "idna"
        version = "3.6"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 }]

        [[distribution]]
        name = "iniconfig"
        version = "2.0.0"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "anyio"

        [distribution.optional-dependencies]

        [[distribution.optional-dependencies.test]]
        name = "iniconfig"

        [[distribution]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 }]
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
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/a1/13/6df5fc090ff4e5d246baf1f45fe9e5623aa8565757dfa5bd243f6a545f9e/blinker-1.7.0.tar.gz", hash = "sha256:e6820ff6fa4e4d1d8e2747c2283749c3f547e4fee112b98555cdcdae32996182", size = 28134 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/fa/2a/7f3714cbc6356a0efec525ce7a0613d581072ed6eb53eb7b9754f33db807/blinker-1.7.0-py3-none-any.whl", hash = "sha256:c3f865d4d54db7abc53758a01601cf343fe55b84c1de4e3fa910e420b438d5b9", size = 13068 }]

        [[distribution]]
        name = "click"
        version = "8.1.7"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941 }]

        [[distribution.dependencies]]
        name = "colorama"
        marker = "platform_system == 'Windows'"

        [[distribution]]
        name = "colorama"
        version = "0.4.6"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/d8/53/6f443c9a4a8358a93a6792e2acffb9d9d5cb0a5cfd8802644b7b1c9a02e4/colorama-0.4.6.tar.gz", hash = "sha256:08695f5cb7ed6e0531a20572697297273c47b8cae5a63ffc6d6ed5c201be6e44", size = 27697 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/d1/d6/3965ed04c63042e047cb6a3e6ed1a63a35087b6a609aa3a15ed8ac56c221/colorama-0.4.6-py2.py3-none-any.whl", hash = "sha256:4f1d9991f5acc0ca119f9d443620b77f9d6b33703e51011c16baf57afb285fc6", size = 25335 }]

        [[distribution]]
        name = "flask"
        version = "3.0.2"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/3f/e0/a89e8120faea1edbfca1a9b171cff7f2bf62ec860bbafcb2c2387c0317be/flask-3.0.2.tar.gz", hash = "sha256:822c03f4b799204250a7ee84b1eddc40665395333973dfb9deebfe425fefcb7d", size = 675248 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/93/a6/aa98bfe0eb9b8b15d36cdfd03c8ca86a03968a87f27ce224fb4f766acb23/flask-3.0.2-py3-none-any.whl", hash = "sha256:3232e0e9c850d781933cf0207523d1ece087eb8d87b23777ae38456e2fbe7c6e", size = 101300 }]

        [[distribution.dependencies]]
        name = "blinker"

        [[distribution.dependencies]]
        name = "click"

        [[distribution.dependencies]]
        name = "itsdangerous"

        [[distribution.dependencies]]
        name = "jinja2"

        [[distribution.dependencies]]
        name = "werkzeug"

        [distribution.optional-dependencies]

        [[distribution.optional-dependencies.dotenv]]
        name = "python-dotenv"

        [[distribution]]
        name = "itsdangerous"
        version = "2.1.2"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/7f/a1/d3fb83e7a61fa0c0d3d08ad0a94ddbeff3731c05212617dff3a94e097f08/itsdangerous-2.1.2.tar.gz", hash = "sha256:5dbbc68b317e5e42f327f9021763545dc3fc3bfe22e6deb96aaf1fc38874156a", size = 56143 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/68/5f/447e04e828f47465eeab35b5d408b7ebaaaee207f48b7136c5a7267a30ae/itsdangerous-2.1.2-py3-none-any.whl", hash = "sha256:2c2349112351b88699d8d4b6b075022c0808887cb7ad10069318a8b0bc88db44", size = 15749 }]

        [[distribution]]
        name = "jinja2"
        version = "3.1.3"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/b2/5e/3a21abf3cd467d7876045335e681d276ac32492febe6d98ad89562d1a7e1/Jinja2-3.1.3.tar.gz", hash = "sha256:ac8bd6544d4bb2c9792bf3a159e80bba8fda7f07e81bc3aed565432d5925ba90", size = 268261 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/30/6d/6de6be2d02603ab56e72997708809e8a5b0fbfee080735109b40a3564843/Jinja2-3.1.3-py3-none-any.whl", hash = "sha256:7d6d50dd97d52cbc355597bd845fabfbac3f551e1f99619e39a35ce8c370b5fa", size = 133236 }]

        [[distribution.dependencies]]
        name = "markupsafe"

        [[distribution]]
        name = "markupsafe"
        version = "2.1.5"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/87/5b/aae44c6655f3801e81aa3eef09dbbf012431987ba564d7231722f68df02d/MarkupSafe-2.1.5.tar.gz", hash = "sha256:d283d37a890ba4c1ae73ffadf8046435c76e7bc2247bbb63c00bd1a709c6544b", size = 19384 }
        wheels = [
        	{ url = "https://files.pythonhosted.org/packages/e4/54/ad5eb37bf9d51800010a74e4665425831a9db4e7c4e0fde4352e391e808e/MarkupSafe-2.1.5-cp310-cp310-macosx_10_9_universal2.whl", hash = "sha256:a17a92de5231666cfbe003f0e4b9b3a7ae3afb1ec2845aadc2bacc93ff85febc", size = 18206 },
        	{ url = "https://files.pythonhosted.org/packages/6a/4a/a4d49415e600bacae038c67f9fecc1d5433b9d3c71a4de6f33537b89654c/MarkupSafe-2.1.5-cp310-cp310-macosx_10_9_x86_64.whl", hash = "sha256:72b6be590cc35924b02c78ef34b467da4ba07e4e0f0454a2c5907f473fc50ce5", size = 14079 },
        	{ url = "https://files.pythonhosted.org/packages/0a/7b/85681ae3c33c385b10ac0f8dd025c30af83c78cec1c37a6aa3b55e67f5ec/MarkupSafe-2.1.5-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:e61659ba32cf2cf1481e575d0462554625196a1f2fc06a1c777d3f48e8865d46", size = 26620 },
        	{ url = "https://files.pythonhosted.org/packages/7c/52/2b1b570f6b8b803cef5ac28fdf78c0da318916c7d2fe9402a84d591b394c/MarkupSafe-2.1.5-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:2174c595a0d73a3080ca3257b40096db99799265e1c27cc5a610743acd86d62f", size = 25818 },
        	{ url = "https://files.pythonhosted.org/packages/29/fe/a36ba8c7ca55621620b2d7c585313efd10729e63ef81e4e61f52330da781/MarkupSafe-2.1.5-cp310-cp310-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ae2ad8ae6ebee9d2d94b17fb62763125f3f374c25618198f40cbb8b525411900", size = 25493 },
        	{ url = "https://files.pythonhosted.org/packages/60/ae/9c60231cdfda003434e8bd27282b1f4e197ad5a710c14bee8bea8a9ca4f0/MarkupSafe-2.1.5-cp310-cp310-musllinux_1_1_aarch64.whl", hash = "sha256:075202fa5b72c86ad32dc7d0b56024ebdbcf2048c0ba09f1cde31bfdd57bcfff", size = 30630 },
        	{ url = "https://files.pythonhosted.org/packages/65/dc/1510be4d179869f5dafe071aecb3f1f41b45d37c02329dfba01ff59e5ac5/MarkupSafe-2.1.5-cp310-cp310-musllinux_1_1_i686.whl", hash = "sha256:598e3276b64aff0e7b3451b72e94fa3c238d452e7ddcd893c3ab324717456bad", size = 29745 },
        	{ url = "https://files.pythonhosted.org/packages/30/39/8d845dd7d0b0613d86e0ef89549bfb5f61ed781f59af45fc96496e897f3a/MarkupSafe-2.1.5-cp310-cp310-musllinux_1_1_x86_64.whl", hash = "sha256:fce659a462a1be54d2ffcacea5e3ba2d74daa74f30f5f143fe0c58636e355fdd", size = 30021 },
        	{ url = "https://files.pythonhosted.org/packages/c7/5c/356a6f62e4f3c5fbf2602b4771376af22a3b16efa74eb8716fb4e328e01e/MarkupSafe-2.1.5-cp310-cp310-win32.whl", hash = "sha256:d9fad5155d72433c921b782e58892377c44bd6252b5af2f67f16b194987338a4", size = 16659 },
        	{ url = "https://files.pythonhosted.org/packages/69/48/acbf292615c65f0604a0c6fc402ce6d8c991276e16c80c46a8f758fbd30c/MarkupSafe-2.1.5-cp310-cp310-win_amd64.whl", hash = "sha256:bf50cd79a75d181c9181df03572cdce0fbb75cc353bc350712073108cba98de5", size = 17213 },
        	{ url = "https://files.pythonhosted.org/packages/11/e7/291e55127bb2ae67c64d66cef01432b5933859dfb7d6949daa721b89d0b3/MarkupSafe-2.1.5-cp311-cp311-macosx_10_9_universal2.whl", hash = "sha256:629ddd2ca402ae6dbedfceeba9c46d5f7b2a61d9749597d4307f943ef198fc1f", size = 18219 },
        	{ url = "https://files.pythonhosted.org/packages/6b/cb/aed7a284c00dfa7c0682d14df85ad4955a350a21d2e3b06d8240497359bf/MarkupSafe-2.1.5-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:5b7b716f97b52c5a14bffdf688f971b2d5ef4029127f1ad7a513973cfd818df2", size = 14098 },
        	{ url = "https://files.pythonhosted.org/packages/1c/cf/35fe557e53709e93feb65575c93927942087e9b97213eabc3fe9d5b25a55/MarkupSafe-2.1.5-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:6ec585f69cec0aa07d945b20805be741395e28ac1627333b1c5b0105962ffced", size = 29014 },
        	{ url = "https://files.pythonhosted.org/packages/97/18/c30da5e7a0e7f4603abfc6780574131221d9148f323752c2755d48abad30/MarkupSafe-2.1.5-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:b91c037585eba9095565a3556f611e3cbfaa42ca1e865f7b8015fe5c7336d5a5", size = 28220 },
        	{ url = "https://files.pythonhosted.org/packages/0c/40/2e73e7d532d030b1e41180807a80d564eda53babaf04d65e15c1cf897e40/MarkupSafe-2.1.5-cp311-cp311-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:7502934a33b54030eaf1194c21c692a534196063db72176b0c4028e140f8f32c", size = 27756 },
        	{ url = "https://files.pythonhosted.org/packages/18/46/5dca760547e8c59c5311b332f70605d24c99d1303dd9a6e1fc3ed0d73561/MarkupSafe-2.1.5-cp311-cp311-musllinux_1_1_aarch64.whl", hash = "sha256:0e397ac966fdf721b2c528cf028494e86172b4feba51d65f81ffd65c63798f3f", size = 33988 },
        	{ url = "https://files.pythonhosted.org/packages/6d/c5/27febe918ac36397919cd4a67d5579cbbfa8da027fa1238af6285bb368ea/MarkupSafe-2.1.5-cp311-cp311-musllinux_1_1_i686.whl", hash = "sha256:c061bb86a71b42465156a3ee7bd58c8c2ceacdbeb95d05a99893e08b8467359a", size = 32718 },
        	{ url = "https://files.pythonhosted.org/packages/f8/81/56e567126a2c2bc2684d6391332e357589a96a76cb9f8e5052d85cb0ead8/MarkupSafe-2.1.5-cp311-cp311-musllinux_1_1_x86_64.whl", hash = "sha256:3a57fdd7ce31c7ff06cdfbf31dafa96cc533c21e443d57f5b1ecc6cdc668ec7f", size = 33317 },
        	{ url = "https://files.pythonhosted.org/packages/00/0b/23f4b2470accb53285c613a3ab9ec19dc944eaf53592cb6d9e2af8aa24cc/MarkupSafe-2.1.5-cp311-cp311-win32.whl", hash = "sha256:397081c1a0bfb5124355710fe79478cdbeb39626492b15d399526ae53422b906", size = 16670 },
        	{ url = "https://files.pythonhosted.org/packages/b7/a2/c78a06a9ec6d04b3445a949615c4c7ed86a0b2eb68e44e7541b9d57067cc/MarkupSafe-2.1.5-cp311-cp311-win_amd64.whl", hash = "sha256:2b7c57a4dfc4f16f7142221afe5ba4e093e09e728ca65c51f5620c9aaeb9a617", size = 17224 },
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
        	{ url = "https://files.pythonhosted.org/packages/a7/88/a940e11827ea1c136a34eca862486178294ae841164475b9ab216b80eb8e/MarkupSafe-2.1.5-cp37-cp37m-macosx_10_9_x86_64.whl", hash = "sha256:c8b29db45f8fe46ad280a7294f5c3ec36dbac9491f2d1c17345be8e69cc5928f", size = 13982 },
        	{ url = "https://files.pythonhosted.org/packages/cb/06/0d28bd178db529c5ac762a625c335a9168a7a23f280b4db9c95e97046145/MarkupSafe-2.1.5-cp37-cp37m-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ec6a563cff360b50eed26f13adc43e61bc0c04d94b8be985e6fb24b81f6dcfdf", size = 26335 },
        	{ url = "https://files.pythonhosted.org/packages/4a/1d/c4f5016f87ced614eacc7d5fb85b25bcc0ff53e8f058d069fc8cbfdc3c7a/MarkupSafe-2.1.5-cp37-cp37m-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:a549b9c31bec33820e885335b451286e2969a2d9e24879f83fe904a5ce59d70a", size = 25557 },
        	{ url = "https://files.pythonhosted.org/packages/b3/fb/c18b8c9fbe69e347fdbf782c6478f1bc77f19a830588daa224236678339b/MarkupSafe-2.1.5-cp37-cp37m-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:4f11aa001c540f62c6166c7726f71f7573b52c68c31f014c25cc7901deea0b52", size = 25245 },
        	{ url = "https://files.pythonhosted.org/packages/2f/69/30d29adcf9d1d931c75001dd85001adad7374381c9c2086154d9f6445be6/MarkupSafe-2.1.5-cp37-cp37m-musllinux_1_1_aarch64.whl", hash = "sha256:7b2e5a267c855eea6b4283940daa6e88a285f5f2a67f2220203786dfa59b37e9", size = 31013 },
        	{ url = "https://files.pythonhosted.org/packages/3a/03/63498d05bd54278b6ca340099e5b52ffb9cdf2ee4f2d9b98246337e21689/MarkupSafe-2.1.5-cp37-cp37m-musllinux_1_1_i686.whl", hash = "sha256:2d2d793e36e230fd32babe143b04cec8a8b3eb8a3122d2aceb4a371e6b09b8df", size = 30178 },
        	{ url = "https://files.pythonhosted.org/packages/68/79/11b4fe15124692f8673b603433e47abca199a08ecd2a4851bfbdc97dc62d/MarkupSafe-2.1.5-cp37-cp37m-musllinux_1_1_x86_64.whl", hash = "sha256:ce409136744f6521e39fd8e2a24c53fa18ad67aa5bc7c2cf83645cce5b5c4e50", size = 30429 },
        	{ url = "https://files.pythonhosted.org/packages/ed/88/408bdbf292eb86f03201c17489acafae8358ba4e120d92358308c15cea7c/MarkupSafe-2.1.5-cp37-cp37m-win32.whl", hash = "sha256:4096e9de5c6fdf43fb4f04c26fb114f61ef0bf2e5604b6ee3019d51b69e8c371", size = 16633 },
        	{ url = "https://files.pythonhosted.org/packages/6c/4c/3577a52eea1880538c435176bc85e5b3379b7ab442327ccd82118550758f/MarkupSafe-2.1.5-cp37-cp37m-win_amd64.whl", hash = "sha256:4275d846e41ecefa46e2015117a9f491e57a71ddd59bbead77e904dc02b1bed2", size = 17215 },
        	{ url = "https://files.pythonhosted.org/packages/f8/ff/2c942a82c35a49df5de3a630ce0a8456ac2969691b230e530ac12314364c/MarkupSafe-2.1.5-cp38-cp38-macosx_10_9_universal2.whl", hash = "sha256:656f7526c69fac7f600bd1f400991cc282b417d17539a1b228617081106feb4a", size = 18192 },
        	{ url = "https://files.pythonhosted.org/packages/4f/14/6f294b9c4f969d0c801a4615e221c1e084722ea6114ab2114189c5b8cbe0/MarkupSafe-2.1.5-cp38-cp38-macosx_10_9_x86_64.whl", hash = "sha256:97cafb1f3cbcd3fd2b6fbfb99ae11cdb14deea0736fc2b0952ee177f2b813a46", size = 14072 },
        	{ url = "https://files.pythonhosted.org/packages/81/d4/fd74714ed30a1dedd0b82427c02fa4deec64f173831ec716da11c51a50aa/MarkupSafe-2.1.5-cp38-cp38-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:1f3fbcb7ef1f16e48246f704ab79d79da8a46891e2da03f8783a5b6fa41a9532", size = 26928 },
        	{ url = "https://files.pythonhosted.org/packages/c7/bd/50319665ce81bb10e90d1cf76f9e1aa269ea6f7fa30ab4521f14d122a3df/MarkupSafe-2.1.5-cp38-cp38-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:fa9db3f79de01457b03d4f01b34cf91bc0048eb2c3846ff26f66687c2f6d16ab", size = 26106 },
        	{ url = "https://files.pythonhosted.org/packages/4c/6f/f2b0f675635b05f6afd5ea03c094557bdb8622fa8e673387444fe8d8e787/MarkupSafe-2.1.5-cp38-cp38-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ffee1f21e5ef0d712f9033568f8344d5da8cc2869dbd08d87c84656e6a2d2f68", size = 25781 },
        	{ url = "https://files.pythonhosted.org/packages/51/e0/393467cf899b34a9d3678e78961c2c8cdf49fb902a959ba54ece01273fb1/MarkupSafe-2.1.5-cp38-cp38-musllinux_1_1_aarch64.whl", hash = "sha256:5dedb4db619ba5a2787a94d877bc8ffc0566f92a01c0ef214865e54ecc9ee5e0", size = 30518 },
        	{ url = "https://files.pythonhosted.org/packages/f6/02/5437e2ad33047290dafced9df741d9efc3e716b75583bbd73a9984f1b6f7/MarkupSafe-2.1.5-cp38-cp38-musllinux_1_1_i686.whl", hash = "sha256:30b600cf0a7ac9234b2638fbc0fb6158ba5bdcdf46aeb631ead21248b9affbc4", size = 29669 },
        	{ url = "https://files.pythonhosted.org/packages/0e/7d/968284145ffd9d726183ed6237c77938c021abacde4e073020f920e060b2/MarkupSafe-2.1.5-cp38-cp38-musllinux_1_1_x86_64.whl", hash = "sha256:8dd717634f5a044f860435c1d8c16a270ddf0ef8588d4887037c5028b859b0c3", size = 29933 },
        	{ url = "https://files.pythonhosted.org/packages/bf/f3/ecb00fc8ab02b7beae8699f34db9357ae49d9f21d4d3de6f305f34fa949e/MarkupSafe-2.1.5-cp38-cp38-win32.whl", hash = "sha256:daa4ee5a243f0f20d528d939d06670a298dd39b1ad5f8a72a4275124a7819eff", size = 16656 },
        	{ url = "https://files.pythonhosted.org/packages/92/21/357205f03514a49b293e214ac39de01fadd0970a6e05e4bf1ddd0ffd0881/MarkupSafe-2.1.5-cp38-cp38-win_amd64.whl", hash = "sha256:619bc166c4f2de5caa5a633b8b7326fbe98e0ccbfacabd87268a2b15ff73a029", size = 17206 },
        	{ url = "https://files.pythonhosted.org/packages/0f/31/780bb297db036ba7b7bbede5e1d7f1e14d704ad4beb3ce53fb495d22bc62/MarkupSafe-2.1.5-cp39-cp39-macosx_10_9_universal2.whl", hash = "sha256:7a68b554d356a91cce1236aa7682dc01df0edba8d043fd1ce607c49dd3c1edcf", size = 18193 },
        	{ url = "https://files.pythonhosted.org/packages/6c/77/d77701bbef72892affe060cdacb7a2ed7fd68dae3b477a8642f15ad3b132/MarkupSafe-2.1.5-cp39-cp39-macosx_10_9_x86_64.whl", hash = "sha256:db0b55e0f3cc0be60c1f19efdde9a637c32740486004f20d1cff53c3c0ece4d2", size = 14073 },
        	{ url = "https://files.pythonhosted.org/packages/d9/a7/1e558b4f78454c8a3a0199292d96159eb4d091f983bc35ef258314fe7269/MarkupSafe-2.1.5-cp39-cp39-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:3e53af139f8579a6d5f7b76549125f0d94d7e630761a2111bc431fd820e163b8", size = 26486 },
        	{ url = "https://files.pythonhosted.org/packages/5f/5a/360da85076688755ea0cceb92472923086993e86b5613bbae9fbc14136b0/MarkupSafe-2.1.5-cp39-cp39-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:17b950fccb810b3293638215058e432159d2b71005c74371d784862b7e4683f3", size = 25685 },
        	{ url = "https://files.pythonhosted.org/packages/6a/18/ae5a258e3401f9b8312f92b028c54d7026a97ec3ab20bfaddbdfa7d8cce8/MarkupSafe-2.1.5-cp39-cp39-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:4c31f53cdae6ecfa91a77820e8b151dba54ab528ba65dfd235c80b086d68a465", size = 25338 },
        	{ url = "https://files.pythonhosted.org/packages/0b/cc/48206bd61c5b9d0129f4d75243b156929b04c94c09041321456fd06a876d/MarkupSafe-2.1.5-cp39-cp39-musllinux_1_1_aarch64.whl", hash = "sha256:bff1b4290a66b490a2f4719358c0cdcd9bafb6b8f061e45c7a2460866bf50c2e", size = 30439 },
        	{ url = "https://files.pythonhosted.org/packages/d1/06/a41c112ab9ffdeeb5f77bc3e331fdadf97fa65e52e44ba31880f4e7f983c/MarkupSafe-2.1.5-cp39-cp39-musllinux_1_1_i686.whl", hash = "sha256:bc1667f8b83f48511b94671e0e441401371dfd0f0a795c7daa4a3cd1dde55bea", size = 29531 },
        	{ url = "https://files.pythonhosted.org/packages/02/8c/ab9a463301a50dab04d5472e998acbd4080597abc048166ded5c7aa768c8/MarkupSafe-2.1.5-cp39-cp39-musllinux_1_1_x86_64.whl", hash = "sha256:5049256f536511ee3f7e1b3f87d1d1209d327e818e6ae1365e8653d7e3abb6a6", size = 29823 },
        	{ url = "https://files.pythonhosted.org/packages/bc/29/9bc18da763496b055d8e98ce476c8e718dcfd78157e17f555ce6dd7d0895/MarkupSafe-2.1.5-cp39-cp39-win32.whl", hash = "sha256:00e046b6dd71aa03a41079792f8473dc494d564611a8f89bbbd7cb93295ebdcf", size = 16658 },
        	{ url = "https://files.pythonhosted.org/packages/f6/f8/4da07de16f10551ca1f640c92b5f316f9394088b183c6a57183df6de5ae4/MarkupSafe-2.1.5-cp39-cp39-win_amd64.whl", hash = "sha256:fa173ec60341d6bb97a89f5ea19c85c5643c1e7dedebc22f5181eb73573142c5", size = 17211 }
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "flask"

        [[distribution.dependencies]]
        name = "flask"
        extra = "dotenv"

        [[distribution]]
        name = "python-dotenv"
        version = "1.0.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/bc/57/e84d88dfe0aec03b7a2d4327012c1627ab5f03652216c63d49846d7a6c58/python-dotenv-1.0.1.tar.gz", hash = "sha256:e324ee90a023d808f1959c46bcbc04446a10ced277783dc6ee09987c37ec10ca", size = 39115 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/6a/3e/b68c118422ec867fa7ab88444e1274aa40681c606d59ac27de5a5588f082/python_dotenv-1.0.1-py3-none-any.whl", hash = "sha256:f7b63ef50f1b690dddf550d03497b66d609393b40b564ed0d674909a68ebf16a", size = 19863 }]

        [[distribution]]
        name = "werkzeug"
        version = "3.0.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/0d/cc/ff1904eb5eb4b455e442834dabf9427331ac0fa02853bf83db817a7dd53d/werkzeug-3.0.1.tar.gz", hash = "sha256:507e811ecea72b18a404947aded4b3390e1db8f826b494d76550ef45bb3b1dcc", size = 801436 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/c3/fc/254c3e9b5feb89ff5b9076a23218dafbc99c96ac5941e900b71206e6313b/werkzeug-3.0.1-py3-none-any.whl", hash = "sha256:90a285dc0e42ad56b34e696398b8122ee4c681833fb35b8334a095d82c56da10", size = 226669 }]

        [[distribution.dependencies]]
        name = "markupsafe"
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
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/71/da/e94e26401b62acd6d91df2b52954aceb7f561743aa5ccc32152886c76c96/certifi-2024.2.2.tar.gz", hash = "sha256:0569859f95fc761b18b45ef421b1290a0f65f147e92a1e5eb3e635f9a5e4e66f", size = 164886 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/ba/06/a07f096c664aeb9f01624f858c3add0a4e913d6c96257acb4fce61e7de14/certifi-2024.2.2-py3-none-any.whl", hash = "sha256:dc383c07b76109f368f6106eee2b593b04a011ea4d55f652c6ca24a754d1cdd1", size = 163774 }]

        [[distribution]]
        name = "charset-normalizer"
        version = "3.0.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/96/d7/1675d9089a1f4677df5eb29c3f8b064aa1e70c1251a0a8a127803158942d/charset-normalizer-3.0.1.tar.gz", hash = "sha256:ebea339af930f8ca5d7a699b921106c6e29c617fe9606fa7baa043c1cdae326f", size = 92842 }
        wheels = [
        	{ url = "https://files.pythonhosted.org/packages/b2/4c/9a4f30042bfee22d34d80daf75f51817cdd23180d718e0160aab235c4faf/charset_normalizer-3.0.1-cp310-cp310-macosx_10_9_universal2.whl", hash = "sha256:88600c72ef7587fe1708fd242b385b6ed4b8904976d5da0893e31df8b3480cb6", size = 201319 },
        	{ url = "https://files.pythonhosted.org/packages/e1/7f/64b51f144fa9e74da63fa690d9563eae627f4df6cc6ae5185a781e1912e5/charset_normalizer-3.0.1-cp310-cp310-macosx_10_9_x86_64.whl", hash = "sha256:c75ffc45f25324e68ab238cb4b5c0a38cd1c3d7f1fb1f72b5541de469e2247db", size = 124210 },
        	{ url = "https://files.pythonhosted.org/packages/f0/78/30d853a3073c866b47abede6d86b5532aa99ac67a95e86077d20be1ce481/charset_normalizer-3.0.1-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:db72b07027db150f468fbada4d85b3b2729a3db39178abf5c543b784c1254539", size = 122454 },
        	{ url = "https://files.pythonhosted.org/packages/a3/4b/f565c852163312a0991c30598f403fd06796a12e408d7839cc46ca8d7f4a/charset_normalizer-3.0.1-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:62595ab75873d50d57323a91dd03e6966eb79c41fa834b7a1661ed043b2d404d", size = 195234 },
        	{ url = "https://files.pythonhosted.org/packages/d3/5b/4031145fcfb9ceaf49dad2fbf9a44e062eb2c08aff36f71d8aafbecf4567/charset_normalizer-3.0.1-cp310-cp310-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:ff6f3db31555657f3163b15a6b7c6938d08df7adbfc9dd13d9d19edad678f1e8", size = 208867 },
        	{ url = "https://files.pythonhosted.org/packages/6e/d7/1d4035fcbf7d0f2e89588a142628355d8d1cd652a227acefb9ec85908cd4/charset_normalizer-3.0.1-cp310-cp310-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:772b87914ff1152b92a197ef4ea40efe27a378606c39446ded52c8f80f79702e", size = 198708 },
        	{ url = "https://files.pythonhosted.org/packages/af/63/2c00ff4e657fb9bb76306ffbc7878fd52067e39716f5e8b0dd5582caf1fa/charset_normalizer-3.0.1-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:70990b9c51340e4044cfc394a81f614f3f90d41397104d226f21e66de668730d", size = 198760 },
        	{ url = "https://files.pythonhosted.org/packages/c1/06/b7b1d3d186e0f288500b8a1161ede6b38a0abbf878c2033d667e815e6bd7/charset_normalizer-3.0.1-cp310-cp310-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:292d5e8ba896bbfd6334b096e34bffb56161c81408d6d036a7dfa6929cff8783", size = 200394 },
        	{ url = "https://files.pythonhosted.org/packages/9f/5a/9dc8932d1e5f8eeaa502e3c3fce91c86be20c04eb3ec202d2b7d74b567e5/charset_normalizer-3.0.1-cp310-cp310-musllinux_1_1_aarch64.whl", hash = "sha256:2edb64ee7bf1ed524a1da60cdcd2e1f6e2b4f66ef7c077680739f1641f62f555", size = 191966 },
        	{ url = "https://files.pythonhosted.org/packages/55/2b/35619e03725bfa4af4a902e1996c9ee8052d6bce005ff79c9bd988820cb4/charset_normalizer-3.0.1-cp310-cp310-musllinux_1_1_i686.whl", hash = "sha256:31a9ddf4718d10ae04d9b18801bd776693487cbb57d74cc3458a7673f6f34639", size = 193974 },
        	{ url = "https://files.pythonhosted.org/packages/2d/02/0f875eb6a1cf347bd3a6098f458f79796aafa3b51090fd7b2784736dc67d/charset_normalizer-3.0.1-cp310-cp310-musllinux_1_1_ppc64le.whl", hash = "sha256:44ba614de5361b3e5278e1241fda3dc1838deed864b50a10d7ce92983797fa76", size = 204483 },
        	{ url = "https://files.pythonhosted.org/packages/92/00/b8dc8dd725297b05f1ab4929c9d7e879f31746131534221c5c8948bc7563/charset_normalizer-3.0.1-cp310-cp310-musllinux_1_1_s390x.whl", hash = "sha256:12db3b2c533c23ab812c2b25934f60383361f8a376ae272665f8e48b88e8e1c6", size = 194756 },
        	{ url = "https://files.pythonhosted.org/packages/98/e4/d4685870fda1cc7c5e29899ec329500460418e54f4f5df76ee520e30689a/charset_normalizer-3.0.1-cp310-cp310-musllinux_1_1_x86_64.whl", hash = "sha256:c512accbd6ff0270939b9ac214b84fb5ada5f0409c44298361b2f5e13f9aed9e", size = 192341 },
        	{ url = "https://files.pythonhosted.org/packages/b6/c2/da108d835354b49aa5c738906e9b6a197b071bc5d77d223f6cd98119172a/charset_normalizer-3.0.1-cp310-cp310-win32.whl", hash = "sha256:502218f52498a36d6bf5ea77081844017bf7982cdbe521ad85e64cabee1b608b", size = 88827 },
        	{ url = "https://files.pythonhosted.org/packages/98/f4/5ca33ee1e0b3412cbd13eae230321a9fe819acf1a99ad6482420fb97cc6b/charset_normalizer-3.0.1-cp310-cp310-win_amd64.whl", hash = "sha256:601f36512f9e28f029d9481bdaf8e89e5148ac5d89cffd3b05cd533eeb423b59", size = 96458 },
        	{ url = "https://files.pythonhosted.org/packages/37/00/ca188e0a2b3cd3184cdd2521b8765cf579327d128caa8aedc3dc7614020a/charset_normalizer-3.0.1-cp311-cp311-macosx_10_9_universal2.whl", hash = "sha256:0298eafff88c99982a4cf66ba2efa1128e4ddaca0b05eec4c456bbc7db691d8d", size = 198711 },
        	{ url = "https://files.pythonhosted.org/packages/90/59/941e2e5ae6828a688c6437ad16e026eb3606d0cfdd13ea5c9090980f3ffd/charset_normalizer-3.0.1-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:a8d0fc946c784ff7f7c3742310cc8a57c5c6dc31631269876a88b809dbeff3d3", size = 122917 },
        	{ url = "https://files.pythonhosted.org/packages/02/49/78b4c1bc8b1b0e0fc66fb31ce30d8302f10a1412ba75de72c57532f0beb0/charset_normalizer-3.0.1-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:87701167f2a5c930b403e9756fab1d31d4d4da52856143b609e30a1ce7160f3c", size = 121106 },
        	{ url = "https://files.pythonhosted.org/packages/c0/4d/6b82099e3f25a9ed87431e2f51156c14f3a9ce8fad73880a3856cd95f1d5/charset_normalizer-3.0.1-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:14e76c0f23218b8f46c4d87018ca2e441535aed3632ca134b10239dfb6dadd6b", size = 193256 },
        	{ url = "https://files.pythonhosted.org/packages/12/e5/aa09a1c39c3e444dd223d63e2c816c18ed78d035cff954143b2a539bdc9e/charset_normalizer-3.0.1-cp311-cp311-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:0c0a590235ccd933d9892c627dec5bc7511ce6ad6c1011fdf5b11363022746c1", size = 206860 },
        	{ url = "https://files.pythonhosted.org/packages/df/c5/dd3a17a615775d0ffc3e12b0e47833d8b7e0a4871431dad87a3f92382a19/charset_normalizer-3.0.1-cp311-cp311-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:8c7fe7afa480e3e82eed58e0ca89f751cd14d767638e2550c77a92a9e749c317", size = 197277 },
        	{ url = "https://files.pythonhosted.org/packages/d9/7a/60d45c9453212b30eebbf8b5cddbdef330eebddfcf335bce7920c43fb72e/charset_normalizer-3.0.1-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:79909e27e8e4fcc9db4addea88aa63f6423ebb171db091fb4373e3312cb6d603", size = 196800 },
        	{ url = "https://files.pythonhosted.org/packages/84/ff/78a4942ef1ea4d1c464cc9a132122b36c5390c5cf6301ed0f9e3e6e24bd9/charset_normalizer-3.0.1-cp311-cp311-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:8ac7b6a045b814cf0c47f3623d21ebd88b3e8cf216a14790b455ea7ff0135d18", size = 198114 },
        	{ url = "https://files.pythonhosted.org/packages/01/ff/9ee4a44e8c32fe96dfc12daa42f29294608a55eadc88f327939327fb20fb/charset_normalizer-3.0.1-cp311-cp311-musllinux_1_1_aarch64.whl", hash = "sha256:72966d1b297c741541ca8cf1223ff262a6febe52481af742036a0b296e35fa5a", size = 189967 },
        	{ url = "https://files.pythonhosted.org/packages/89/87/c237a299a658b35d19fd531eeb8247480627fc2fb4b7a471334b48850f45/charset_normalizer-3.0.1-cp311-cp311-musllinux_1_1_i686.whl", hash = "sha256:f9d0c5c045a3ca9bedfc35dca8526798eb91a07aa7a2c0fee134c6c6f321cbd7", size = 191806 },
        	{ url = "https://files.pythonhosted.org/packages/86/eb/31c9025b4ed7eddd930c5f2ac269efb953de33140608c7539675d74a2081/charset_normalizer-3.0.1-cp311-cp311-musllinux_1_1_ppc64le.whl", hash = "sha256:5995f0164fa7df59db4746112fec3f49c461dd6b31b841873443bdb077c13cfc", size = 202257 },
        	{ url = "https://files.pythonhosted.org/packages/80/54/183163f9910936e57a60ee618f4f5cc91c2f8333ee2d4ebc6c50f6c8684d/charset_normalizer-3.0.1-cp311-cp311-musllinux_1_1_s390x.whl", hash = "sha256:4a8fcf28c05c1f6d7e177a9a46a1c52798bfe2ad80681d275b10dcf317deaf0b", size = 192933 },
        	{ url = "https://files.pythonhosted.org/packages/82/49/ab81421d5aa25bc8535896a017c93204cb4051f2a4e72b1ad8f3b594e072/charset_normalizer-3.0.1-cp311-cp311-musllinux_1_1_x86_64.whl", hash = "sha256:761e8904c07ad053d285670f36dd94e1b6ab7f16ce62b9805c475b7aa1cffde6", size = 190554 },
        	{ url = "https://files.pythonhosted.org/packages/84/0e/5965dd90991e4f2588718b865115a78c8b040193ac3676f757b7fb6af9d0/charset_normalizer-3.0.1-cp311-cp311-win32.whl", hash = "sha256:71140351489970dfe5e60fc621ada3e0f41104a5eddaca47a7acb3c1b851d6d3", size = 88588 },
        	{ url = "https://files.pythonhosted.org/packages/2e/7b/5053a4a46fac017fd2aea3dc9abdd9983fd4cef153b6eb6aedcb0d7cb6e3/charset_normalizer-3.0.1-cp311-cp311-win_amd64.whl", hash = "sha256:9ab77acb98eba3fd2a85cd160851816bfce6871d944d885febf012713f06659c", size = 96042 },
        	{ url = "https://files.pythonhosted.org/packages/d2/7f/3c8a6db3eda16ce79a01552ec85ac8fd0ea6265976eb4db250a60b7416ab/charset_normalizer-3.0.1-cp36-cp36m-macosx_10_9_x86_64.whl", hash = "sha256:84c3990934bae40ea69a82034912ffe5a62c60bbf6ec5bc9691419641d7d5c9a", size = 116359 },
        	{ url = "https://files.pythonhosted.org/packages/a0/98/7b0d3a853af59e092cdd77c7e1c67ca92fd6acc126285240dbb552b4162f/charset_normalizer-3.0.1-cp36-cp36m-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:74292fc76c905c0ef095fe11e188a32ebd03bc38f3f3e9bcb85e4e6db177b7ea", size = 158659 },
        	{ url = "https://files.pythonhosted.org/packages/46/69/9f42514a9f58c602ab89a2af89081a475dccd959f9bc01ba7e61372d31bd/charset_normalizer-3.0.1-cp36-cp36m-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:c95a03c79bbe30eec3ec2b7f076074f4281526724c8685a42872974ef4d36b72", size = 168598 },
        	{ url = "https://files.pythonhosted.org/packages/f1/14/ed5990189a6a25ae9f8d63e74cd0336189f9ad7e51f066ba2f6cb73e8126/charset_normalizer-3.0.1-cp36-cp36m-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:f4c39b0e3eac288fedc2b43055cfc2ca7a60362d0e5e87a637beac5d801ef478", size = 160332 },
        	{ url = "https://files.pythonhosted.org/packages/9c/94/1725fc3e0dbe8918a4ec6dd317ec1ef388e701bdfb5053e1f34f5c6d5a8e/charset_normalizer-3.0.1-cp36-cp36m-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:df2c707231459e8a4028eabcd3cfc827befd635b3ef72eada84ab13b52e1574d", size = 162997 },
        	{ url = "https://files.pythonhosted.org/packages/97/f9/366db2d2cf69d641159d6448b813ac9b1b5f9807a46fde6c50b36c1387f8/charset_normalizer-3.0.1-cp36-cp36m-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:93ad6d87ac18e2a90b0fe89df7c65263b9a99a0eb98f0a3d2e079f12a0735837", size = 164721 },
        	{ url = "https://files.pythonhosted.org/packages/90/2c/bb5e4f7e2e9871793b5c0fb5c6c4056458a148a05143143320f2d4a410a9/charset_normalizer-3.0.1-cp36-cp36m-musllinux_1_1_aarch64.whl", hash = "sha256:59e5686dd847347e55dffcc191a96622f016bc0ad89105e24c14e0d6305acbc6", size = 155823 },
        	{ url = "https://files.pythonhosted.org/packages/67/c6/cf4e8a8f41201284bdf200f764b29a87f6f7d22fe3c9eddab602af489acc/charset_normalizer-3.0.1-cp36-cp36m-musllinux_1_1_i686.whl", hash = "sha256:cd6056167405314a4dc3c173943f11249fa0f1b204f8b51ed4bde1a9cd1834dc", size = 160754 },
        	{ url = "https://files.pythonhosted.org/packages/27/b1/8dfcfa5d9978b845466cd41973b3d714eba3926fcb50f6fcddd45cfb75a2/charset_normalizer-3.0.1-cp36-cp36m-musllinux_1_1_ppc64le.whl", hash = "sha256:083c8d17153ecb403e5e1eb76a7ef4babfc2c48d58899c98fcaa04833e7a2f9a", size = 165193 },
        	{ url = "https://files.pythonhosted.org/packages/8f/e2/73ea48d2608f71a879588b607e093d550b8eaa177eb31bbdf1c01e515818/charset_normalizer-3.0.1-cp36-cp36m-musllinux_1_1_s390x.whl", hash = "sha256:f5057856d21e7586765171eac8b9fc3f7d44ef39425f85dbcccb13b3ebea806c", size = 158155 },
        	{ url = "https://files.pythonhosted.org/packages/35/86/d85885ed7ac236a297b0b8beab5f0703fc0516f803ddf7b1910f255b83f3/charset_normalizer-3.0.1-cp36-cp36m-musllinux_1_1_x86_64.whl", hash = "sha256:7eb33a30d75562222b64f569c642ff3dc6689e09adda43a082208397f016c39a", size = 158613 },
        	{ url = "https://files.pythonhosted.org/packages/9a/bf/c9fa15ccf216a69aaaa735c961d7fac2a2801a1b01023fe05d194bf076b4/charset_normalizer-3.0.1-cp36-cp36m-win32.whl", hash = "sha256:95dea361dd73757c6f1c0a1480ac499952c16ac83f7f5f4f84f0658a01b8ef41", size = 85267 },
        	{ url = "https://files.pythonhosted.org/packages/0b/8b/3cf0eff3c8b6734cd4336c23a3141846d579931a31e6476c8091961f1e25/charset_normalizer-3.0.1-cp36-cp36m-win_amd64.whl", hash = "sha256:eaa379fcd227ca235d04152ca6704c7cb55564116f8bc52545ff357628e10602", size = 91314 },
        	{ url = "https://files.pythonhosted.org/packages/16/bd/671f11f920dfb46de848e9176d84ddb25b3bbdffac6751cbbf691c0b5b17/charset_normalizer-3.0.1-cp37-cp37m-macosx_10_9_x86_64.whl", hash = "sha256:3e45867f1f2ab0711d60c6c71746ac53537f1684baa699f4f668d4c6f6ce8e14", size = 120446 },
        	{ url = "https://files.pythonhosted.org/packages/df/2f/4806e155191f75e720aca98a969581c6b2676f0379dd315c34c388bbf8b5/charset_normalizer-3.0.1-cp37-cp37m-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:cadaeaba78750d58d3cc6ac4d1fd867da6fc73c88156b7a3212a3cd4819d679d", size = 166934 },
        	{ url = "https://files.pythonhosted.org/packages/25/b5/f477e419b06e49f3bae446cbdc1fd71d2599be8b12b4d45c641c5a4495b1/charset_normalizer-3.0.1-cp37-cp37m-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:911d8a40b2bef5b8bbae2e36a0b103f142ac53557ab421dc16ac4aafee6f53dc", size = 177082 },
        	{ url = "https://files.pythonhosted.org/packages/0f/45/f462f534dd2853ebbc186ed859661db454665b1dc9ae6c690d982153cda9/charset_normalizer-3.0.1-cp37-cp37m-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:503e65837c71b875ecdd733877d852adbc465bd82c768a067badd953bf1bc5a3", size = 168207 },
        	{ url = "https://files.pythonhosted.org/packages/c1/b2/d81606aebeb7e9a33dc877ff3a206c9946f5bb374c99d22d4a28825aa270/charset_normalizer-3.0.1-cp37-cp37m-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:a60332922359f920193b1d4826953c507a877b523b2395ad7bc716ddd386d866", size = 170531 },
        	{ url = "https://files.pythonhosted.org/packages/71/67/79be03bf7ab4198d994c2e8da869ca354487bfa25656b95cf289cf6338a2/charset_normalizer-3.0.1-cp37-cp37m-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:16a8663d6e281208d78806dbe14ee9903715361cf81f6d4309944e4d1e59ac5b", size = 172361 },
        	{ url = "https://files.pythonhosted.org/packages/a2/93/0b1aa4dbc0ae2aa2e1b2e6d037ab8984dc09912d6b26d63ced14da07e3a7/charset_normalizer-3.0.1-cp37-cp37m-musllinux_1_1_aarch64.whl", hash = "sha256:a16418ecf1329f71df119e8a65f3aa68004a3f9383821edcb20f0702934d8087", size = 163201 },
        	{ url = "https://files.pythonhosted.org/packages/00/35/830c29e5dab61932224c7a6c89427090164a3e425cf03486ce7a3ce60623/charset_normalizer-3.0.1-cp37-cp37m-musllinux_1_1_i686.whl", hash = "sha256:9d9153257a3f70d5f69edf2325357251ed20f772b12e593f3b3377b5f78e7ef8", size = 168391 },
        	{ url = "https://files.pythonhosted.org/packages/93/d1/569445a704414e150f198737c245ab96b40d28d5b68045a62c414a5157de/charset_normalizer-3.0.1-cp37-cp37m-musllinux_1_1_ppc64le.whl", hash = "sha256:02a51034802cbf38db3f89c66fb5d2ec57e6fe7ef2f4a44d070a593c3688667b", size = 173512 },
        	{ url = "https://files.pythonhosted.org/packages/a3/09/a837b27b122e710dfad15b0b5df04cd0623c8d8d3382e4298f50798fb84a/charset_normalizer-3.0.1-cp37-cp37m-musllinux_1_1_s390x.whl", hash = "sha256:2e396d70bc4ef5325b72b593a72c8979999aa52fb8bcf03f701c1b03e1166918", size = 165357 },
        	{ url = "https://files.pythonhosted.org/packages/5a/d8/9e76846e70e729de85ecc6af21edc584a2adfef202dc5f5ae00a02622e3d/charset_normalizer-3.0.1-cp37-cp37m-musllinux_1_1_x86_64.whl", hash = "sha256:11b53acf2411c3b09e6af37e4b9005cba376c872503c8f28218c7243582df45d", size = 166261 },
        	{ url = "https://files.pythonhosted.org/packages/0e/fd/0d099502582af039ef8a8c954d69d7dadbe5f425cb1b24d175eb0034ea9e/charset_normalizer-3.0.1-cp37-cp37m-win32.whl", hash = "sha256:0bf2dae5291758b6f84cf923bfaa285632816007db0330002fa1de38bfcb7154", size = 87576 },
        	{ url = "https://files.pythonhosted.org/packages/fc/64/443267b7824283b3e0e33cee4240c079939a970c2c9a5a3164fc988d690b/charset_normalizer-3.0.1-cp37-cp37m-win_amd64.whl", hash = "sha256:2c03cc56021a4bd59be889c2b9257dae13bf55041a3372d3295416f86b295fb5", size = 94003 },
        	{ url = "https://files.pythonhosted.org/packages/0e/d3/c5fa421dc69bb77c581ed561f1ec6656109c97731ad1128aa93d8bad3053/charset_normalizer-3.0.1-cp38-cp38-macosx_10_9_universal2.whl", hash = "sha256:024e606be3ed92216e2b6952ed859d86b4cfa52cd5bc5f050e7dc28f9b43ec42", size = 198947 },
        	{ url = "https://files.pythonhosted.org/packages/9c/42/c1ebc736c57459aab28bfb8aa28c6a047796f2ea46050a3b129b4920dbe4/charset_normalizer-3.0.1-cp38-cp38-macosx_10_9_x86_64.whl", hash = "sha256:4b0d02d7102dd0f997580b51edc4cebcf2ab6397a7edf89f1c73b586c614272c", size = 122896 },
        	{ url = "https://files.pythonhosted.org/packages/a2/a7/adc963ad8f8fddadd6be088e636972705ec9d1d92d1b45e6119eb02b7e9e/charset_normalizer-3.0.1-cp38-cp38-macosx_11_0_arm64.whl", hash = "sha256:358a7c4cb8ba9b46c453b1dd8d9e431452d5249072e4f56cfda3149f6ab1405e", size = 121341 },
        	{ url = "https://files.pythonhosted.org/packages/17/da/fdf8ffc33716c82cae06008159a55a581fa515e8dd02e3395dcad42ff83d/charset_normalizer-3.0.1-cp38-cp38-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:81d6741ab457d14fdedc215516665050f3822d3e56508921cc7239f8c8e66a58", size = 192140 },
        	{ url = "https://files.pythonhosted.org/packages/37/60/7a01f3a129d1af1f26ab2c56aae89a72dbf33fd46a467c1aa994ec62b90b/charset_normalizer-3.0.1-cp38-cp38-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:8b8af03d2e37866d023ad0ddea594edefc31e827fee64f8de5611a1dbc373174", size = 205254 },
        	{ url = "https://files.pythonhosted.org/packages/56/5d/275fb120957dfe5a2262d04f28bc742fd4bcc2bd270d19bb8757e09737ef/charset_normalizer-3.0.1-cp38-cp38-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:9cf4e8ad252f7c38dd1f676b46514f92dc0ebeb0db5552f5f403509705e24753", size = 195236 },
        	{ url = "https://files.pythonhosted.org/packages/20/a2/16b2cbf5f73bdd10624b94647b85c008ba25059792a5c7b4fdb8358bceeb/charset_normalizer-3.0.1-cp38-cp38-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:e696f0dd336161fca9adbb846875d40752e6eba585843c768935ba5c9960722b", size = 195413 },
        	{ url = "https://files.pythonhosted.org/packages/c8/a2/8f873138c99423de3b402daf8ccd7a538632c83d0c129444a6a18ef34e03/charset_normalizer-3.0.1-cp38-cp38-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:c22d3fe05ce11d3671297dc8973267daa0f938b93ec716e12e0f6dee81591dc1", size = 197834 },
        	{ url = "https://files.pythonhosted.org/packages/5b/e7/5527effca09d873e07e128d3daac7c531203b5105cb4e2956c2b7a8cc41c/charset_normalizer-3.0.1-cp38-cp38-musllinux_1_1_aarch64.whl", hash = "sha256:109487860ef6a328f3eec66f2bf78b0b72400280d8f8ea05f69c51644ba6521a", size = 188766 },
        	{ url = "https://files.pythonhosted.org/packages/e8/80/141f6af05332cbb811ab469f64deb1e1d4cc9e8b0c003aa8a38d689ce84a/charset_normalizer-3.0.1-cp38-cp38-musllinux_1_1_i686.whl", hash = "sha256:37f8febc8ec50c14f3ec9637505f28e58d4f66752207ea177c1d67df25da5aed", size = 193337 },
        	{ url = "https://files.pythonhosted.org/packages/aa/a4/2d6255d4db5d4558a92458fd8dacddfdda2fb4ad9c0a87db6f6034aded34/charset_normalizer-3.0.1-cp38-cp38-musllinux_1_1_ppc64le.whl", hash = "sha256:f97e83fa6c25693c7a35de154681fcc257c1c41b38beb0304b9c4d2d9e164479", size = 201134 },
        	{ url = "https://files.pythonhosted.org/packages/f1/ff/9a1c65d8c44958f45ae40cd558ab63bd499a35198a2014e13c0887c07ed1/charset_normalizer-3.0.1-cp38-cp38-musllinux_1_1_s390x.whl", hash = "sha256:a152f5f33d64a6be73f1d30c9cc82dfc73cec6477ec268e7c6e4c7d23c2d2291", size = 193470 },
        	{ url = "https://files.pythonhosted.org/packages/03/5e/e81488c74e86eef85cf085417ed945da2dcca87ed22d76202680c16bd3c3/charset_normalizer-3.0.1-cp38-cp38-musllinux_1_1_x86_64.whl", hash = "sha256:39049da0ffb96c8cbb65cbf5c5f3ca3168990adf3551bd1dee10c48fce8ae820", size = 190966 },
        	{ url = "https://files.pythonhosted.org/packages/3a/91/a233f06d33dc3ac90a9991d238fbc68c59615d9f71be1801e14ac4e42d7f/charset_normalizer-3.0.1-cp38-cp38-win32.whl", hash = "sha256:4457ea6774b5611f4bed5eaa5df55f70abde42364d498c5134b7ef4c6958e20e", size = 88649 },
        	{ url = "https://files.pythonhosted.org/packages/87/5d/0ebaee2249a04fd20bb4baeb9ea2c29dee17317175d9d67b4f5f34cf048d/charset_normalizer-3.0.1-cp38-cp38-win_amd64.whl", hash = "sha256:e62164b50f84e20601c1ff8eb55620d2ad25fb81b59e3cd776a1902527a788af", size = 95762 },
        	{ url = "https://files.pythonhosted.org/packages/17/67/4b25c0358a2e812312b551e734d58855d58f47d0e0e9d1573930003910cb/charset_normalizer-3.0.1-cp39-cp39-macosx_10_9_universal2.whl", hash = "sha256:8eade758719add78ec36dc13201483f8e9b5d940329285edcd5f70c0a9edbd7f", size = 201301 },
        	{ url = "https://files.pythonhosted.org/packages/6a/ab/3a00ecbddabe25132c20c1bd45e6f90c537b5f7a0b5bcaba094c4922928c/charset_normalizer-3.0.1-cp39-cp39-macosx_10_9_x86_64.whl", hash = "sha256:8499ca8f4502af841f68135133d8258f7b32a53a1d594aa98cc52013fff55678", size = 124025 },
        	{ url = "https://files.pythonhosted.org/packages/f5/84/cac681144a28114bd9e40d3cdbfd961c14ecc2b56f1baec2094afd6744c7/charset_normalizer-3.0.1-cp39-cp39-macosx_11_0_arm64.whl", hash = "sha256:3fc1c4a2ffd64890aebdb3f97e1278b0cc72579a08ca4de8cd2c04799a3a22be", size = 122437 },
        	{ url = "https://files.pythonhosted.org/packages/b5/1a/932d86fde86bb0d2992c74552c9a422883fe0890132bbc9a5e2211f03318/charset_normalizer-3.0.1-cp39-cp39-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:00d3ffdaafe92a5dc603cb9bd5111aaa36dfa187c8285c543be562e61b755f6b", size = 195601 },
        	{ url = "https://files.pythonhosted.org/packages/dc/ff/2c7655d83b1d6d6a0e132d50d54131fcb8da763b417ccc6c4a506aa0e08c/charset_normalizer-3.0.1-cp39-cp39-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:c2ac1b08635a8cd4e0cbeaf6f5e922085908d48eb05d44c5ae9eabab148512ca", size = 208758 },
        	{ url = "https://files.pythonhosted.org/packages/31/af/67b7653a35dbd56f6bb9ff54652a551eae8420d1d0545f0042c5bdb15fb0/charset_normalizer-3.0.1-cp39-cp39-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:f6f45710b4459401609ebebdbcfb34515da4fc2aa886f95107f556ac69a9147e", size = 198892 },
        	{ url = "https://files.pythonhosted.org/packages/e3/96/8cdbce165c96cce5f2c9c7748f7ed8e0cf0c5d03e213bbc90b7c3e918bf5/charset_normalizer-3.0.1-cp39-cp39-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:3ae1de54a77dc0d6d5fcf623290af4266412a7c4be0b1ff7444394f03f5c54e3", size = 198769 },
        	{ url = "https://files.pythonhosted.org/packages/99/24/eb846dc9a797da58e6e5b3b5a71d3ff17264de3f424fb29aaa5d27173b55/charset_normalizer-3.0.1-cp39-cp39-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:3b590df687e3c5ee0deef9fc8c547d81986d9a1b56073d82de008744452d6541", size = 200586 },
        	{ url = "https://files.pythonhosted.org/packages/31/06/f6330ee70c041a032ee1a5d32785d69748cfa41f64b6d327cc08cae51de9/charset_normalizer-3.0.1-cp39-cp39-musllinux_1_1_aarch64.whl", hash = "sha256:ab5de034a886f616a5668aa5d098af2b5385ed70142090e2a31bcbd0af0fdb3d", size = 192171 },
        	{ url = "https://files.pythonhosted.org/packages/c4/d4/94f1ea460cce04483d2460efba6fd4d66e6f60ad6fc6075dba13e3501e48/charset_normalizer-3.0.1-cp39-cp39-musllinux_1_1_i686.whl", hash = "sha256:9cb3032517f1627cc012dbc80a8ec976ae76d93ea2b5feaa9d2a5b8882597579", size = 194195 },
        	{ url = "https://files.pythonhosted.org/packages/c9/dd/80a5e8c080b7e1cc2b0ca35f0d6aeedafd7bbd06d25031ac20868b1366d6/charset_normalizer-3.0.1-cp39-cp39-musllinux_1_1_ppc64le.whl", hash = "sha256:608862a7bf6957f2333fc54ab4399e405baad0163dc9f8d99cb236816db169d4", size = 204208 },
        	{ url = "https://files.pythonhosted.org/packages/25/19/298089cef2eb82fd3810d982aa239d4226594f99e1fe78494cb9b47b03c9/charset_normalizer-3.0.1-cp39-cp39-musllinux_1_1_s390x.whl", hash = "sha256:0f438ae3532723fb6ead77e7c604be7c8374094ef4ee2c5e03a3a17f1fca256c", size = 195019 },
        	{ url = "https://files.pythonhosted.org/packages/f5/ec/a9bed59079bd0267d34ada58a4048c96a59b3621e7f586ea85840d41831d/charset_normalizer-3.0.1-cp39-cp39-musllinux_1_1_x86_64.whl", hash = "sha256:356541bf4381fa35856dafa6a965916e54bed415ad8a24ee6de6e37deccf2786", size = 192318 },
        	{ url = "https://files.pythonhosted.org/packages/6e/a3/997ff79260f76210b1d73463b9081ae7edbf16ff3d611b67f5e72c685cab/charset_normalizer-3.0.1-cp39-cp39-win32.whl", hash = "sha256:39cf9ed17fe3b1bc81f33c9ceb6ce67683ee7526e65fde1447c772afc54a1bb8", size = 88851 },
        	{ url = "https://files.pythonhosted.org/packages/e7/0d/5eaceb5abfc000cca204af9f50e9839462dc0bb1c4e0f4b14ed370e3febd/charset_normalizer-3.0.1-cp39-cp39-win_amd64.whl", hash = "sha256:0a11e971ed097d24c534c037d298ad32c6ce81a45736d31e0ff0ad37ab437d59", size = 96457 },
        	{ url = "https://files.pythonhosted.org/packages/68/2b/02e9d6a98ddb73fa238d559a9edcc30b247b8dc4ee848b6184c936e99dc0/charset_normalizer-3.0.1-py3-none-any.whl", hash = "sha256:7e189e2e1d3ed2f4aebabd2d5b0f931e883676e51c7624826e0a4e5fe8a0bf24", size = 45489 }
        ]

        [[distribution]]
        name = "idna"
        version = "3.6"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "requests"

        [[distribution.dependencies]]
        name = "requests"
        extra = "socks"
        marker = "python_version < '3.10'"

        [[distribution]]
        name = "pysocks"
        version = "1.7.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/bd/11/293dd436aea955d45fc4e8a35b6ae7270f5b8e00b53cf6c024c83b657a11/PySocks-1.7.1.tar.gz", hash = "sha256:3f8804571ebe159c380ac6de37643bb4685970655d3bba243530d6558b799aa0", size = 284429 }
        wheels = [
        	{ url = "https://files.pythonhosted.org/packages/a2/4b/52123768624ae28d84c97515dd96c9958888e8c2d8f122074e31e2be878c/PySocks-1.7.1-py27-none-any.whl", hash = "sha256:08e69f092cc6dbe92a0fdd16eeb9b9ffbc13cadfe5ca4c7bd92ffb078b293299", size = 16726 },
        	{ url = "https://files.pythonhosted.org/packages/8d/59/b4572118e098ac8e46e399a1dd0f2d85403ce8bbaad9ec79373ed6badaf9/PySocks-1.7.1-py3-none-any.whl", hash = "sha256:2725bd0a9925919b9b51739eea5f9e2bae91e83288108a9ad338b2e3a4435ee5", size = 16725 }
        ]

        [[distribution]]
        name = "requests"
        version = "2.31.0"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/9d/be/10918a2eac4ae9f02f6cfe6414b7a155ccd8f7f9d4380d62fd5b955065c3/requests-2.31.0.tar.gz", hash = "sha256:942c5a758f98d790eaed1a29cb6eefc7ffb0d1cf7af05c3d2791656dbd6ad1e1", size = 110794 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/70/8e/0e2d847013cb52cd35b38c009bb167a1a26b2ce6cd6965bf26b47bc0bf44/requests-2.31.0-py3-none-any.whl", hash = "sha256:58cd2187c01e70e6e26505bca751777aa9f2ee0b7f4300988b709f44e013003f", size = 62574 }]

        [[distribution.dependencies]]
        name = "certifi"

        [[distribution.dependencies]]
        name = "charset-normalizer"

        [[distribution.dependencies]]
        name = "idna"

        [[distribution.dependencies]]
        name = "urllib3"

        [distribution.optional-dependencies]

        [[distribution.optional-dependencies.socks]]
        name = "pysocks"

        [[distribution]]
        name = "urllib3"
        version = "2.0.7"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/af/47/b215df9f71b4fdba1025fc05a77db2ad243fa0926755a52c5e71659f4e3c/urllib3-2.0.7.tar.gz", hash = "sha256:c97dfde1f7bd43a71c8d2a58e369e9b2bf692d1334ea9f9cae55add7d0dd0f84", size = 282546 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/d2/b2/b157855192a68541a91ba7b2bbcb91f1b4faa51f8bae38d8005c034be524/urllib3-2.0.7-py3-none-any.whl", hash = "sha256:fdb6d215c776278489906c2f8916e6e7d4f5a9b602ccbcfdf7f016fc8da0596e", size = 124213 }]
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
     + charset-normalizer==3.0.1
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
     + charset-normalizer==3.0.1
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
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/a1/13/6df5fc090ff4e5d246baf1f45fe9e5623aa8565757dfa5bd243f6a545f9e/blinker-1.7.0.tar.gz", hash = "sha256:e6820ff6fa4e4d1d8e2747c2283749c3f547e4fee112b98555cdcdae32996182", size = 28134 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/fa/2a/7f3714cbc6356a0efec525ce7a0613d581072ed6eb53eb7b9754f33db807/blinker-1.7.0-py3-none-any.whl", hash = "sha256:c3f865d4d54db7abc53758a01601cf343fe55b84c1de4e3fa910e420b438d5b9", size = 13068 }]

        [[distribution]]
        name = "click"
        version = "8.1.7"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941 }]

        [[distribution.dependencies]]
        name = "colorama"
        marker = "platform_system == 'Windows'"

        [[distribution]]
        name = "colorama"
        version = "0.4.6"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/d8/53/6f443c9a4a8358a93a6792e2acffb9d9d5cb0a5cfd8802644b7b1c9a02e4/colorama-0.4.6.tar.gz", hash = "sha256:08695f5cb7ed6e0531a20572697297273c47b8cae5a63ffc6d6ed5c201be6e44", size = 27697 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/d1/d6/3965ed04c63042e047cb6a3e6ed1a63a35087b6a609aa3a15ed8ac56c221/colorama-0.4.6-py2.py3-none-any.whl", hash = "sha256:4f1d9991f5acc0ca119f9d443620b77f9d6b33703e51011c16baf57afb285fc6", size = 25335 }]

        [[distribution]]
        name = "flask"
        version = "3.0.2"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/3f/e0/a89e8120faea1edbfca1a9b171cff7f2bf62ec860bbafcb2c2387c0317be/flask-3.0.2.tar.gz", hash = "sha256:822c03f4b799204250a7ee84b1eddc40665395333973dfb9deebfe425fefcb7d", size = 675248 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/93/a6/aa98bfe0eb9b8b15d36cdfd03c8ca86a03968a87f27ce224fb4f766acb23/flask-3.0.2-py3-none-any.whl", hash = "sha256:3232e0e9c850d781933cf0207523d1ece087eb8d87b23777ae38456e2fbe7c6e", size = 101300 }]

        [[distribution.dependencies]]
        name = "blinker"

        [[distribution.dependencies]]
        name = "click"

        [[distribution.dependencies]]
        name = "itsdangerous"

        [[distribution.dependencies]]
        name = "jinja2"

        [[distribution.dependencies]]
        name = "werkzeug"

        [[distribution]]
        name = "itsdangerous"
        version = "2.1.2"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/7f/a1/d3fb83e7a61fa0c0d3d08ad0a94ddbeff3731c05212617dff3a94e097f08/itsdangerous-2.1.2.tar.gz", hash = "sha256:5dbbc68b317e5e42f327f9021763545dc3fc3bfe22e6deb96aaf1fc38874156a", size = 56143 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/68/5f/447e04e828f47465eeab35b5d408b7ebaaaee207f48b7136c5a7267a30ae/itsdangerous-2.1.2-py3-none-any.whl", hash = "sha256:2c2349112351b88699d8d4b6b075022c0808887cb7ad10069318a8b0bc88db44", size = 15749 }]

        [[distribution]]
        name = "jinja2"
        version = "3.1.3"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/b2/5e/3a21abf3cd467d7876045335e681d276ac32492febe6d98ad89562d1a7e1/Jinja2-3.1.3.tar.gz", hash = "sha256:ac8bd6544d4bb2c9792bf3a159e80bba8fda7f07e81bc3aed565432d5925ba90", size = 268261 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/30/6d/6de6be2d02603ab56e72997708809e8a5b0fbfee080735109b40a3564843/Jinja2-3.1.3-py3-none-any.whl", hash = "sha256:7d6d50dd97d52cbc355597bd845fabfbac3f551e1f99619e39a35ce8c370b5fa", size = 133236 }]

        [[distribution.dependencies]]
        name = "markupsafe"

        [[distribution]]
        name = "markupsafe"
        version = "2.1.5"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/87/5b/aae44c6655f3801e81aa3eef09dbbf012431987ba564d7231722f68df02d/MarkupSafe-2.1.5.tar.gz", hash = "sha256:d283d37a890ba4c1ae73ffadf8046435c76e7bc2247bbb63c00bd1a709c6544b", size = 19384 }
        wheels = [
        	{ url = "https://files.pythonhosted.org/packages/e4/54/ad5eb37bf9d51800010a74e4665425831a9db4e7c4e0fde4352e391e808e/MarkupSafe-2.1.5-cp310-cp310-macosx_10_9_universal2.whl", hash = "sha256:a17a92de5231666cfbe003f0e4b9b3a7ae3afb1ec2845aadc2bacc93ff85febc", size = 18206 },
        	{ url = "https://files.pythonhosted.org/packages/6a/4a/a4d49415e600bacae038c67f9fecc1d5433b9d3c71a4de6f33537b89654c/MarkupSafe-2.1.5-cp310-cp310-macosx_10_9_x86_64.whl", hash = "sha256:72b6be590cc35924b02c78ef34b467da4ba07e4e0f0454a2c5907f473fc50ce5", size = 14079 },
        	{ url = "https://files.pythonhosted.org/packages/0a/7b/85681ae3c33c385b10ac0f8dd025c30af83c78cec1c37a6aa3b55e67f5ec/MarkupSafe-2.1.5-cp310-cp310-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:e61659ba32cf2cf1481e575d0462554625196a1f2fc06a1c777d3f48e8865d46", size = 26620 },
        	{ url = "https://files.pythonhosted.org/packages/7c/52/2b1b570f6b8b803cef5ac28fdf78c0da318916c7d2fe9402a84d591b394c/MarkupSafe-2.1.5-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:2174c595a0d73a3080ca3257b40096db99799265e1c27cc5a610743acd86d62f", size = 25818 },
        	{ url = "https://files.pythonhosted.org/packages/29/fe/a36ba8c7ca55621620b2d7c585313efd10729e63ef81e4e61f52330da781/MarkupSafe-2.1.5-cp310-cp310-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ae2ad8ae6ebee9d2d94b17fb62763125f3f374c25618198f40cbb8b525411900", size = 25493 },
        	{ url = "https://files.pythonhosted.org/packages/60/ae/9c60231cdfda003434e8bd27282b1f4e197ad5a710c14bee8bea8a9ca4f0/MarkupSafe-2.1.5-cp310-cp310-musllinux_1_1_aarch64.whl", hash = "sha256:075202fa5b72c86ad32dc7d0b56024ebdbcf2048c0ba09f1cde31bfdd57bcfff", size = 30630 },
        	{ url = "https://files.pythonhosted.org/packages/65/dc/1510be4d179869f5dafe071aecb3f1f41b45d37c02329dfba01ff59e5ac5/MarkupSafe-2.1.5-cp310-cp310-musllinux_1_1_i686.whl", hash = "sha256:598e3276b64aff0e7b3451b72e94fa3c238d452e7ddcd893c3ab324717456bad", size = 29745 },
        	{ url = "https://files.pythonhosted.org/packages/30/39/8d845dd7d0b0613d86e0ef89549bfb5f61ed781f59af45fc96496e897f3a/MarkupSafe-2.1.5-cp310-cp310-musllinux_1_1_x86_64.whl", hash = "sha256:fce659a462a1be54d2ffcacea5e3ba2d74daa74f30f5f143fe0c58636e355fdd", size = 30021 },
        	{ url = "https://files.pythonhosted.org/packages/c7/5c/356a6f62e4f3c5fbf2602b4771376af22a3b16efa74eb8716fb4e328e01e/MarkupSafe-2.1.5-cp310-cp310-win32.whl", hash = "sha256:d9fad5155d72433c921b782e58892377c44bd6252b5af2f67f16b194987338a4", size = 16659 },
        	{ url = "https://files.pythonhosted.org/packages/69/48/acbf292615c65f0604a0c6fc402ce6d8c991276e16c80c46a8f758fbd30c/MarkupSafe-2.1.5-cp310-cp310-win_amd64.whl", hash = "sha256:bf50cd79a75d181c9181df03572cdce0fbb75cc353bc350712073108cba98de5", size = 17213 },
        	{ url = "https://files.pythonhosted.org/packages/11/e7/291e55127bb2ae67c64d66cef01432b5933859dfb7d6949daa721b89d0b3/MarkupSafe-2.1.5-cp311-cp311-macosx_10_9_universal2.whl", hash = "sha256:629ddd2ca402ae6dbedfceeba9c46d5f7b2a61d9749597d4307f943ef198fc1f", size = 18219 },
        	{ url = "https://files.pythonhosted.org/packages/6b/cb/aed7a284c00dfa7c0682d14df85ad4955a350a21d2e3b06d8240497359bf/MarkupSafe-2.1.5-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:5b7b716f97b52c5a14bffdf688f971b2d5ef4029127f1ad7a513973cfd818df2", size = 14098 },
        	{ url = "https://files.pythonhosted.org/packages/1c/cf/35fe557e53709e93feb65575c93927942087e9b97213eabc3fe9d5b25a55/MarkupSafe-2.1.5-cp311-cp311-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:6ec585f69cec0aa07d945b20805be741395e28ac1627333b1c5b0105962ffced", size = 29014 },
        	{ url = "https://files.pythonhosted.org/packages/97/18/c30da5e7a0e7f4603abfc6780574131221d9148f323752c2755d48abad30/MarkupSafe-2.1.5-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:b91c037585eba9095565a3556f611e3cbfaa42ca1e865f7b8015fe5c7336d5a5", size = 28220 },
        	{ url = "https://files.pythonhosted.org/packages/0c/40/2e73e7d532d030b1e41180807a80d564eda53babaf04d65e15c1cf897e40/MarkupSafe-2.1.5-cp311-cp311-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:7502934a33b54030eaf1194c21c692a534196063db72176b0c4028e140f8f32c", size = 27756 },
        	{ url = "https://files.pythonhosted.org/packages/18/46/5dca760547e8c59c5311b332f70605d24c99d1303dd9a6e1fc3ed0d73561/MarkupSafe-2.1.5-cp311-cp311-musllinux_1_1_aarch64.whl", hash = "sha256:0e397ac966fdf721b2c528cf028494e86172b4feba51d65f81ffd65c63798f3f", size = 33988 },
        	{ url = "https://files.pythonhosted.org/packages/6d/c5/27febe918ac36397919cd4a67d5579cbbfa8da027fa1238af6285bb368ea/MarkupSafe-2.1.5-cp311-cp311-musllinux_1_1_i686.whl", hash = "sha256:c061bb86a71b42465156a3ee7bd58c8c2ceacdbeb95d05a99893e08b8467359a", size = 32718 },
        	{ url = "https://files.pythonhosted.org/packages/f8/81/56e567126a2c2bc2684d6391332e357589a96a76cb9f8e5052d85cb0ead8/MarkupSafe-2.1.5-cp311-cp311-musllinux_1_1_x86_64.whl", hash = "sha256:3a57fdd7ce31c7ff06cdfbf31dafa96cc533c21e443d57f5b1ecc6cdc668ec7f", size = 33317 },
        	{ url = "https://files.pythonhosted.org/packages/00/0b/23f4b2470accb53285c613a3ab9ec19dc944eaf53592cb6d9e2af8aa24cc/MarkupSafe-2.1.5-cp311-cp311-win32.whl", hash = "sha256:397081c1a0bfb5124355710fe79478cdbeb39626492b15d399526ae53422b906", size = 16670 },
        	{ url = "https://files.pythonhosted.org/packages/b7/a2/c78a06a9ec6d04b3445a949615c4c7ed86a0b2eb68e44e7541b9d57067cc/MarkupSafe-2.1.5-cp311-cp311-win_amd64.whl", hash = "sha256:2b7c57a4dfc4f16f7142221afe5ba4e093e09e728ca65c51f5620c9aaeb9a617", size = 17224 },
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
        	{ url = "https://files.pythonhosted.org/packages/a7/88/a940e11827ea1c136a34eca862486178294ae841164475b9ab216b80eb8e/MarkupSafe-2.1.5-cp37-cp37m-macosx_10_9_x86_64.whl", hash = "sha256:c8b29db45f8fe46ad280a7294f5c3ec36dbac9491f2d1c17345be8e69cc5928f", size = 13982 },
        	{ url = "https://files.pythonhosted.org/packages/cb/06/0d28bd178db529c5ac762a625c335a9168a7a23f280b4db9c95e97046145/MarkupSafe-2.1.5-cp37-cp37m-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ec6a563cff360b50eed26f13adc43e61bc0c04d94b8be985e6fb24b81f6dcfdf", size = 26335 },
        	{ url = "https://files.pythonhosted.org/packages/4a/1d/c4f5016f87ced614eacc7d5fb85b25bcc0ff53e8f058d069fc8cbfdc3c7a/MarkupSafe-2.1.5-cp37-cp37m-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:a549b9c31bec33820e885335b451286e2969a2d9e24879f83fe904a5ce59d70a", size = 25557 },
        	{ url = "https://files.pythonhosted.org/packages/b3/fb/c18b8c9fbe69e347fdbf782c6478f1bc77f19a830588daa224236678339b/MarkupSafe-2.1.5-cp37-cp37m-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:4f11aa001c540f62c6166c7726f71f7573b52c68c31f014c25cc7901deea0b52", size = 25245 },
        	{ url = "https://files.pythonhosted.org/packages/2f/69/30d29adcf9d1d931c75001dd85001adad7374381c9c2086154d9f6445be6/MarkupSafe-2.1.5-cp37-cp37m-musllinux_1_1_aarch64.whl", hash = "sha256:7b2e5a267c855eea6b4283940daa6e88a285f5f2a67f2220203786dfa59b37e9", size = 31013 },
        	{ url = "https://files.pythonhosted.org/packages/3a/03/63498d05bd54278b6ca340099e5b52ffb9cdf2ee4f2d9b98246337e21689/MarkupSafe-2.1.5-cp37-cp37m-musllinux_1_1_i686.whl", hash = "sha256:2d2d793e36e230fd32babe143b04cec8a8b3eb8a3122d2aceb4a371e6b09b8df", size = 30178 },
        	{ url = "https://files.pythonhosted.org/packages/68/79/11b4fe15124692f8673b603433e47abca199a08ecd2a4851bfbdc97dc62d/MarkupSafe-2.1.5-cp37-cp37m-musllinux_1_1_x86_64.whl", hash = "sha256:ce409136744f6521e39fd8e2a24c53fa18ad67aa5bc7c2cf83645cce5b5c4e50", size = 30429 },
        	{ url = "https://files.pythonhosted.org/packages/ed/88/408bdbf292eb86f03201c17489acafae8358ba4e120d92358308c15cea7c/MarkupSafe-2.1.5-cp37-cp37m-win32.whl", hash = "sha256:4096e9de5c6fdf43fb4f04c26fb114f61ef0bf2e5604b6ee3019d51b69e8c371", size = 16633 },
        	{ url = "https://files.pythonhosted.org/packages/6c/4c/3577a52eea1880538c435176bc85e5b3379b7ab442327ccd82118550758f/MarkupSafe-2.1.5-cp37-cp37m-win_amd64.whl", hash = "sha256:4275d846e41ecefa46e2015117a9f491e57a71ddd59bbead77e904dc02b1bed2", size = 17215 },
        	{ url = "https://files.pythonhosted.org/packages/f8/ff/2c942a82c35a49df5de3a630ce0a8456ac2969691b230e530ac12314364c/MarkupSafe-2.1.5-cp38-cp38-macosx_10_9_universal2.whl", hash = "sha256:656f7526c69fac7f600bd1f400991cc282b417d17539a1b228617081106feb4a", size = 18192 },
        	{ url = "https://files.pythonhosted.org/packages/4f/14/6f294b9c4f969d0c801a4615e221c1e084722ea6114ab2114189c5b8cbe0/MarkupSafe-2.1.5-cp38-cp38-macosx_10_9_x86_64.whl", hash = "sha256:97cafb1f3cbcd3fd2b6fbfb99ae11cdb14deea0736fc2b0952ee177f2b813a46", size = 14072 },
        	{ url = "https://files.pythonhosted.org/packages/81/d4/fd74714ed30a1dedd0b82427c02fa4deec64f173831ec716da11c51a50aa/MarkupSafe-2.1.5-cp38-cp38-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:1f3fbcb7ef1f16e48246f704ab79d79da8a46891e2da03f8783a5b6fa41a9532", size = 26928 },
        	{ url = "https://files.pythonhosted.org/packages/c7/bd/50319665ce81bb10e90d1cf76f9e1aa269ea6f7fa30ab4521f14d122a3df/MarkupSafe-2.1.5-cp38-cp38-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:fa9db3f79de01457b03d4f01b34cf91bc0048eb2c3846ff26f66687c2f6d16ab", size = 26106 },
        	{ url = "https://files.pythonhosted.org/packages/4c/6f/f2b0f675635b05f6afd5ea03c094557bdb8622fa8e673387444fe8d8e787/MarkupSafe-2.1.5-cp38-cp38-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ffee1f21e5ef0d712f9033568f8344d5da8cc2869dbd08d87c84656e6a2d2f68", size = 25781 },
        	{ url = "https://files.pythonhosted.org/packages/51/e0/393467cf899b34a9d3678e78961c2c8cdf49fb902a959ba54ece01273fb1/MarkupSafe-2.1.5-cp38-cp38-musllinux_1_1_aarch64.whl", hash = "sha256:5dedb4db619ba5a2787a94d877bc8ffc0566f92a01c0ef214865e54ecc9ee5e0", size = 30518 },
        	{ url = "https://files.pythonhosted.org/packages/f6/02/5437e2ad33047290dafced9df741d9efc3e716b75583bbd73a9984f1b6f7/MarkupSafe-2.1.5-cp38-cp38-musllinux_1_1_i686.whl", hash = "sha256:30b600cf0a7ac9234b2638fbc0fb6158ba5bdcdf46aeb631ead21248b9affbc4", size = 29669 },
        	{ url = "https://files.pythonhosted.org/packages/0e/7d/968284145ffd9d726183ed6237c77938c021abacde4e073020f920e060b2/MarkupSafe-2.1.5-cp38-cp38-musllinux_1_1_x86_64.whl", hash = "sha256:8dd717634f5a044f860435c1d8c16a270ddf0ef8588d4887037c5028b859b0c3", size = 29933 },
        	{ url = "https://files.pythonhosted.org/packages/bf/f3/ecb00fc8ab02b7beae8699f34db9357ae49d9f21d4d3de6f305f34fa949e/MarkupSafe-2.1.5-cp38-cp38-win32.whl", hash = "sha256:daa4ee5a243f0f20d528d939d06670a298dd39b1ad5f8a72a4275124a7819eff", size = 16656 },
        	{ url = "https://files.pythonhosted.org/packages/92/21/357205f03514a49b293e214ac39de01fadd0970a6e05e4bf1ddd0ffd0881/MarkupSafe-2.1.5-cp38-cp38-win_amd64.whl", hash = "sha256:619bc166c4f2de5caa5a633b8b7326fbe98e0ccbfacabd87268a2b15ff73a029", size = 17206 },
        	{ url = "https://files.pythonhosted.org/packages/0f/31/780bb297db036ba7b7bbede5e1d7f1e14d704ad4beb3ce53fb495d22bc62/MarkupSafe-2.1.5-cp39-cp39-macosx_10_9_universal2.whl", hash = "sha256:7a68b554d356a91cce1236aa7682dc01df0edba8d043fd1ce607c49dd3c1edcf", size = 18193 },
        	{ url = "https://files.pythonhosted.org/packages/6c/77/d77701bbef72892affe060cdacb7a2ed7fd68dae3b477a8642f15ad3b132/MarkupSafe-2.1.5-cp39-cp39-macosx_10_9_x86_64.whl", hash = "sha256:db0b55e0f3cc0be60c1f19efdde9a637c32740486004f20d1cff53c3c0ece4d2", size = 14073 },
        	{ url = "https://files.pythonhosted.org/packages/d9/a7/1e558b4f78454c8a3a0199292d96159eb4d091f983bc35ef258314fe7269/MarkupSafe-2.1.5-cp39-cp39-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:3e53af139f8579a6d5f7b76549125f0d94d7e630761a2111bc431fd820e163b8", size = 26486 },
        	{ url = "https://files.pythonhosted.org/packages/5f/5a/360da85076688755ea0cceb92472923086993e86b5613bbae9fbc14136b0/MarkupSafe-2.1.5-cp39-cp39-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:17b950fccb810b3293638215058e432159d2b71005c74371d784862b7e4683f3", size = 25685 },
        	{ url = "https://files.pythonhosted.org/packages/6a/18/ae5a258e3401f9b8312f92b028c54d7026a97ec3ab20bfaddbdfa7d8cce8/MarkupSafe-2.1.5-cp39-cp39-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:4c31f53cdae6ecfa91a77820e8b151dba54ab528ba65dfd235c80b086d68a465", size = 25338 },
        	{ url = "https://files.pythonhosted.org/packages/0b/cc/48206bd61c5b9d0129f4d75243b156929b04c94c09041321456fd06a876d/MarkupSafe-2.1.5-cp39-cp39-musllinux_1_1_aarch64.whl", hash = "sha256:bff1b4290a66b490a2f4719358c0cdcd9bafb6b8f061e45c7a2460866bf50c2e", size = 30439 },
        	{ url = "https://files.pythonhosted.org/packages/d1/06/a41c112ab9ffdeeb5f77bc3e331fdadf97fa65e52e44ba31880f4e7f983c/MarkupSafe-2.1.5-cp39-cp39-musllinux_1_1_i686.whl", hash = "sha256:bc1667f8b83f48511b94671e0e441401371dfd0f0a795c7daa4a3cd1dde55bea", size = 29531 },
        	{ url = "https://files.pythonhosted.org/packages/02/8c/ab9a463301a50dab04d5472e998acbd4080597abc048166ded5c7aa768c8/MarkupSafe-2.1.5-cp39-cp39-musllinux_1_1_x86_64.whl", hash = "sha256:5049256f536511ee3f7e1b3f87d1d1209d327e818e6ae1365e8653d7e3abb6a6", size = 29823 },
        	{ url = "https://files.pythonhosted.org/packages/bc/29/9bc18da763496b055d8e98ce476c8e718dcfd78157e17f555ce6dd7d0895/MarkupSafe-2.1.5-cp39-cp39-win32.whl", hash = "sha256:00e046b6dd71aa03a41079792f8473dc494d564611a8f89bbbd7cb93295ebdcf", size = 16658 },
        	{ url = "https://files.pythonhosted.org/packages/f6/f8/4da07de16f10551ca1f640c92b5f316f9394088b183c6a57183df6de5ae4/MarkupSafe-2.1.5-cp39-cp39-win_amd64.whl", hash = "sha256:fa173ec60341d6bb97a89f5ea19c85c5643c1e7dedebc22f5181eb73573142c5", size = 17211 }
        ]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "flask"

        [[distribution]]
        name = "werkzeug"
        version = "3.0.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/0d/cc/ff1904eb5eb4b455e442834dabf9427331ac0fa02853bf83db817a7dd53d/werkzeug-3.0.1.tar.gz", hash = "sha256:507e811ecea72b18a404947aded4b3390e1db8f826b494d76550ef45bb3b1dcc", size = 801436 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/c3/fc/254c3e9b5feb89ff5b9076a23218dafbc99c96ac5941e900b71206e6313b/werkzeug-3.0.1-py3-none-any.whl", hash = "sha256:90a285dc0e42ad56b34e696398b8122ee4c681833fb35b8334a095d82c56da10", size = 226669 }]

        [[distribution.dependencies]]
        name = "markupsafe"
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
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/23/a2/97899f6bd0e873fed3a7e67ae8d3a08b21799430fb4da15cfedf10d6e2c2/iniconfig-1.1.1.tar.gz", hash = "sha256:bc3af051d7d14b2ee5ef9969666def0cd1a000e121eaea580d4a313df4b37f32", size = 8104 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl", hash = "sha256:011e24c64b7f47f6ebd835bb12a743f2fbe9a26d4cecaa7f53bc4f35ee9da8b3", size = 4990 }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "iniconfig"
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
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/23/a2/97899f6bd0e873fed3a7e67ae8d3a08b21799430fb4da15cfedf10d6e2c2/iniconfig-1.1.1.tar.gz", hash = "sha256:bc3af051d7d14b2ee5ef9969666def0cd1a000e121eaea580d4a313df4b37f32", size = 8104 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl", hash = "sha256:011e24c64b7f47f6ebd835bb12a743f2fbe9a26d4cecaa7f53bc4f35ee9da8b3", size = 4990 }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "iniconfig"
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
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "iniconfig"
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
        source = "editable+."

        [[distribution.dependencies]]
        name = "uv-public-pypackage"

        [[distribution]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = "git+https://github.com/astral-test/uv-public-pypackage?rev=0dacfd662c64cb4ceb16e6cf65a157a8b715b979#0dacfd662c64cb4ceb16e6cf65a157a8b715b979"
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
        source = "editable+."

        [[distribution.dependencies]]
        name = "uv-public-pypackage"

        [[distribution]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = "git+https://github.com/astral-test/uv-public-pypackage?rev=main#0dacfd662c64cb4ceb16e6cf65a157a8b715b979"
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
        source = "editable+."

        [[distribution.dependencies]]
        name = "uv-public-pypackage"

        [[distribution]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = "git+https://github.com/astral-test/uv-public-pypackage?rev=main#b270df1a2fb5d012294e9aaf05e7e0bab1e6a389"
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

          hint: The `Requires-Python` requirement (>=3.7) defined in your `pyproject.toml` includes Python versions that are not supported by your dependencies (e.g., pygls>=1.1.0,<=1.2.1 only supports >=3.7.9, <4). Consider using a more restrictive `Requires-Python` requirement (like >=3.7.9, <4).
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
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/e3/fc/f800d51204003fa8ae392c4e8278f256206e7a919b708eef054f5f4b650d/attrs-23.2.0.tar.gz", hash = "sha256:935dc3b529c262f6cf76e50877d35a4bd3c1de194fd41f47a2b7ae8f19971f30", size = 780820 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/e0/44/827b2a91a5816512fcaf3cc4ebc465ccd5d598c45cefa6703fcf4a79018f/attrs-23.2.0-py3-none-any.whl", hash = "sha256:99b87a485a5820b23b879f04c2305b44b951b502fd64be915879d77a7e8fc6f1", size = 60752 }]

        [[distribution.dependencies]]
        name = "importlib-metadata"
        marker = "python_version < '3.8'"

        [[distribution]]
        name = "cattrs"
        version = "23.1.2"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/68/d4/27f9fd840e74d51b6d6a024d39ff495b56ffde71d28eb82758b7b85d0617/cattrs-23.1.2.tar.gz", hash = "sha256:db1c821b8c537382b2c7c66678c3790091ca0275ac486c76f3c8f3920e83c657", size = 39998 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/3a/ba/05df14efaa0624fac6b1510e87f5ce446208d2f6ce50270a89b6268aebfe/cattrs-23.1.2-py3-none-any.whl", hash = "sha256:b2bb14311ac17bed0d58785e5a60f022e5431aca3932e3fc5cc8ed8639de50a4", size = 50845 }]

        [[distribution.dependencies]]
        name = "attrs"

        [[distribution.dependencies]]
        name = "exceptiongroup"
        marker = "python_version < '3.11'"

        [[distribution.dependencies]]
        name = "typing-extensions"
        marker = "python_version < '3.11'"

        [[distribution]]
        name = "exceptiongroup"
        version = "1.2.0"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/8e/1c/beef724eaf5b01bb44b6338c8c3494eff7cab376fab4904cfbbc3585dc79/exceptiongroup-1.2.0.tar.gz", hash = "sha256:91f5c769735f051a4290d52edd0858999b57e5876e9f85937691bd4c9fa3ed68", size = 26264 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/b8/9a/5028fd52db10e600f1c4674441b968cf2ea4959085bfb5b99fb1250e5f68/exceptiongroup-1.2.0-py3-none-any.whl", hash = "sha256:4bfd3996ac73b41e9b9628b04e079f193850720ea5945fc96a08633c66912f14", size = 16210 }]

        [[distribution]]
        name = "importlib-metadata"
        version = "6.7.0"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/a3/82/f6e29c8d5c098b6be61460371c2c5591f4a335923639edec43b3830650a4/importlib_metadata-6.7.0.tar.gz", hash = "sha256:1aaf550d4f73e5d6783e7acb77aec43d49da8017410afae93822cc9cca98c4d4", size = 53569 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/ff/94/64287b38c7de4c90683630338cf28f129decbba0a44f0c6db35a873c73c4/importlib_metadata-6.7.0-py3-none-any.whl", hash = "sha256:cb52082e659e97afc5dac71e79de97d8681de3aa07ff18578330904a9d18e5b5", size = 22934 }]

        [[distribution.dependencies]]
        name = "typing-extensions"
        marker = "python_version < '3.8'"

        [[distribution.dependencies]]
        name = "zipp"

        [[distribution]]
        name = "lsprotocol"
        version = "2023.0.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/9d/f6/6e80484ec078d0b50699ceb1833597b792a6c695f90c645fbaf54b947e6f/lsprotocol-2023.0.1.tar.gz", hash = "sha256:cc5c15130d2403c18b734304339e51242d3018a05c4f7d0f198ad6e0cd21861d", size = 69434 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/8d/37/2351e48cb3309673492d3a8c59d407b75fb6630e560eb27ecd4da03adc9a/lsprotocol-2023.0.1-py3-none-any.whl", hash = "sha256:c75223c9e4af2f24272b14c6375787438279369236cd568f596d4951052a60f2", size = 70826 }]

        [[distribution.dependencies]]
        name = "attrs"

        [[distribution.dependencies]]
        name = "cattrs"

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "pygls"

        [[distribution]]
        name = "pygls"
        version = "1.0.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/8e/27/58ff0f76b379fc11a1d03e8d4b4e96fd0abb463d27709a7fb4193bcdbbc4/pygls-1.0.1.tar.gz", hash = "sha256:f3ee98ddbb4690eb5c755bc32ba7e129607f14cbd313575f33d0cea443b78cb2", size = 674546 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/da/9b/4fd77a060068f2f3f46f97ed6ba8762c5a73f11ef0c196cfd34f3a9be878/pygls-1.0.1-py3-none-any.whl", hash = "sha256:adacc96da77598c70f46acfdfd1481d3da90cd54f639f7eee52eb6e4dbd57b55", size = 40367 }]

        [[distribution.dependencies]]
        name = "lsprotocol"

        [[distribution.dependencies]]
        name = "typeguard"

        [[distribution]]
        name = "typeguard"
        version = "2.13.3"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/3a/38/c61bfcf62a7b572b5e9363a802ff92559cb427ee963048e1442e3aef7490/typeguard-2.13.3.tar.gz", hash = "sha256:00edaa8da3a133674796cf5ea87d9f4b4c367d77476e185e80251cc13dfbb8c4", size = 40604 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/9a/bb/d43e5c75054e53efce310e79d63df0ac3f25e34c926be5dffb7d283fb2a8/typeguard-2.13.3-py3-none-any.whl", hash = "sha256:5e3e3be01e887e7eafae5af63d1f36c849aaa94e3a0112097312aabfa16284f1", size = 17605 }]

        [[distribution]]
        name = "typing-extensions"
        version = "4.7.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/3c/8b/0111dd7d6c1478bf83baa1cab85c686426c7a6274119aceb2bd9d35395ad/typing_extensions-4.7.1.tar.gz", hash = "sha256:b75ddc264f0ba5615db7ba217daeb99701ad295353c45f9e95963337ceeeffb2", size = 72876 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/ec/6b/63cc3df74987c36fe26157ee12e09e8f9db4de771e0f3404263117e75b95/typing_extensions-4.7.1-py3-none-any.whl", hash = "sha256:440d5dd3af93b060174bf433bccd69b0babc3b15b1a8dca43789fd7f61514b36", size = 33232 }]

        [[distribution]]
        name = "zipp"
        version = "3.15.0"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/00/27/f0ac6b846684cecce1ee93d32450c45ab607f65c2e0255f0092032d91f07/zipp-3.15.0.tar.gz", hash = "sha256:112929ad649da941c23de50f356a2b5570c954b65150642bccdd66bf194d224b", size = 18454 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/5b/fa/c9e82bbe1af6266adf08afb563905eb87cab83fde00a0a08963510621047/zipp-3.15.0-py3-none-any.whl", hash = "sha256:48904fc76a60e542af151aded95726c1a5c34ed43ab4134b597665c86d7ad556", size = 6758 }]
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
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/e3/fc/f800d51204003fa8ae392c4e8278f256206e7a919b708eef054f5f4b650d/attrs-23.2.0.tar.gz", hash = "sha256:935dc3b529c262f6cf76e50877d35a4bd3c1de194fd41f47a2b7ae8f19971f30", size = 780820 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/e0/44/827b2a91a5816512fcaf3cc4ebc465ccd5d598c45cefa6703fcf4a79018f/attrs-23.2.0-py3-none-any.whl", hash = "sha256:99b87a485a5820b23b879f04c2305b44b951b502fd64be915879d77a7e8fc6f1", size = 60752 }]

        [[distribution.dependencies]]
        name = "importlib-metadata"
        marker = "python_version < '3.8'"

        [[distribution]]
        name = "cattrs"
        version = "23.1.2"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/68/d4/27f9fd840e74d51b6d6a024d39ff495b56ffde71d28eb82758b7b85d0617/cattrs-23.1.2.tar.gz", hash = "sha256:db1c821b8c537382b2c7c66678c3790091ca0275ac486c76f3c8f3920e83c657", size = 39998 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/3a/ba/05df14efaa0624fac6b1510e87f5ce446208d2f6ce50270a89b6268aebfe/cattrs-23.1.2-py3-none-any.whl", hash = "sha256:b2bb14311ac17bed0d58785e5a60f022e5431aca3932e3fc5cc8ed8639de50a4", size = 50845 }]

        [[distribution.dependencies]]
        name = "attrs"

        [[distribution.dependencies]]
        name = "exceptiongroup"
        marker = "python_version < '3.11'"

        [[distribution.dependencies]]
        name = "typing-extensions"
        marker = "python_version < '3.11'"

        [[distribution]]
        name = "exceptiongroup"
        version = "1.2.0"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/8e/1c/beef724eaf5b01bb44b6338c8c3494eff7cab376fab4904cfbbc3585dc79/exceptiongroup-1.2.0.tar.gz", hash = "sha256:91f5c769735f051a4290d52edd0858999b57e5876e9f85937691bd4c9fa3ed68", size = 26264 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/b8/9a/5028fd52db10e600f1c4674441b968cf2ea4959085bfb5b99fb1250e5f68/exceptiongroup-1.2.0-py3-none-any.whl", hash = "sha256:4bfd3996ac73b41e9b9628b04e079f193850720ea5945fc96a08633c66912f14", size = 16210 }]

        [[distribution]]
        name = "importlib-metadata"
        version = "6.7.0"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/a3/82/f6e29c8d5c098b6be61460371c2c5591f4a335923639edec43b3830650a4/importlib_metadata-6.7.0.tar.gz", hash = "sha256:1aaf550d4f73e5d6783e7acb77aec43d49da8017410afae93822cc9cca98c4d4", size = 53569 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/ff/94/64287b38c7de4c90683630338cf28f129decbba0a44f0c6db35a873c73c4/importlib_metadata-6.7.0-py3-none-any.whl", hash = "sha256:cb52082e659e97afc5dac71e79de97d8681de3aa07ff18578330904a9d18e5b5", size = 22934 }]

        [[distribution.dependencies]]
        name = "typing-extensions"
        marker = "python_version < '3.8'"

        [[distribution.dependencies]]
        name = "zipp"

        [[distribution]]
        name = "lsprotocol"
        version = "2023.0.0"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/3e/fe/f7671a4fb28606ff1663bba60aff6af21b1e43a977c74c33db13cb83680f/lsprotocol-2023.0.0.tar.gz", hash = "sha256:c9d92e12a3f4ed9317d3068226592860aab5357d93cf5b2451dc244eee8f35f2", size = 69399 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/2d/5b/f18eb1823a4cee9bed70cdcc25eed5a75845367c42e63a79010a7c34f8a7/lsprotocol-2023.0.0-py3-none-any.whl", hash = "sha256:e85fc87ee26c816adca9eb497bb3db1a7c79c477a11563626e712eaccf926a05", size = 70789 }]

        [[distribution.dependencies]]
        name = "attrs"

        [[distribution.dependencies]]
        name = "cattrs"

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "pygls"

        [[distribution]]
        name = "pygls"
        version = "1.2.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/e6/94/534c11ba5475df09542e48d751a66e0448d52bbbb92cbef5541deef7760d/pygls-1.2.1.tar.gz", hash = "sha256:04f9b9c115b622dcc346fb390289066565343d60245a424eca77cb429b911ed8", size = 45274 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/36/31/3799444d3f072ffca1a35eb02a48f964384cc13f001125e87d9f0748687b/pygls-1.2.1-py3-none-any.whl", hash = "sha256:7dcfcf12b6f15beb606afa46de2ed348b65a279c340ef2242a9a35c22eeafe94", size = 55983 }]

        [[distribution.dependencies]]
        name = "lsprotocol"

        [[distribution]]
        name = "typing-extensions"
        version = "4.7.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/3c/8b/0111dd7d6c1478bf83baa1cab85c686426c7a6274119aceb2bd9d35395ad/typing_extensions-4.7.1.tar.gz", hash = "sha256:b75ddc264f0ba5615db7ba217daeb99701ad295353c45f9e95963337ceeeffb2", size = 72876 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/ec/6b/63cc3df74987c36fe26157ee12e09e8f9db4de771e0f3404263117e75b95/typing_extensions-4.7.1-py3-none-any.whl", hash = "sha256:440d5dd3af93b060174bf433bccd69b0babc3b15b1a8dca43789fd7f61514b36", size = 33232 }]

        [[distribution]]
        name = "zipp"
        version = "3.15.0"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/00/27/f0ac6b846684cecce1ee93d32450c45ab607f65c2e0255f0092032d91f07/zipp-3.15.0.tar.gz", hash = "sha256:112929ad649da941c23de50f356a2b5570c954b65150642bccdd66bf194d224b", size = 18454 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/5b/fa/c9e82bbe1af6266adf08afb563905eb87cab83fde00a0a08963510621047/zipp-3.15.0-py3-none-any.whl", hash = "sha256:48904fc76a60e542af151aded95726c1a5c34ed43ab4134b597665c86d7ad556", size = 6758 }]
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
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/e3/fc/f800d51204003fa8ae392c4e8278f256206e7a919b708eef054f5f4b650d/attrs-23.2.0.tar.gz", hash = "sha256:935dc3b529c262f6cf76e50877d35a4bd3c1de194fd41f47a2b7ae8f19971f30", size = 780820 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/e0/44/827b2a91a5816512fcaf3cc4ebc465ccd5d598c45cefa6703fcf4a79018f/attrs-23.2.0-py3-none-any.whl", hash = "sha256:99b87a485a5820b23b879f04c2305b44b951b502fd64be915879d77a7e8fc6f1", size = 60752 }]

        [[distribution]]
        name = "cattrs"
        version = "23.2.3"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/1e/57/c6ccd22658c4bcb3beb3f1c262e1f170cf136e913b122763d0ddd328d284/cattrs-23.2.3.tar.gz", hash = "sha256:a934090d95abaa9e911dac357e3a8699e0b4b14f8529bcc7d2b1ad9d51672b9f", size = 610215 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/b3/0d/cd4a4071c7f38385dc5ba91286723b4d1090b87815db48216212c6c6c30e/cattrs-23.2.3-py3-none-any.whl", hash = "sha256:0341994d94971052e9ee70662542699a3162ea1e0c62f7ce1b4a57f563685108", size = 57474 }]

        [[distribution.dependencies]]
        name = "attrs"

        [[distribution]]
        name = "lsprotocol"
        version = "2023.0.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/9d/f6/6e80484ec078d0b50699ceb1833597b792a6c695f90c645fbaf54b947e6f/lsprotocol-2023.0.1.tar.gz", hash = "sha256:cc5c15130d2403c18b734304339e51242d3018a05c4f7d0f198ad6e0cd21861d", size = 69434 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/8d/37/2351e48cb3309673492d3a8c59d407b75fb6630e560eb27ecd4da03adc9a/lsprotocol-2023.0.1-py3-none-any.whl", hash = "sha256:c75223c9e4af2f24272b14c6375787438279369236cd568f596d4951052a60f2", size = 70826 }]

        [[distribution.dependencies]]
        name = "attrs"

        [[distribution.dependencies]]
        name = "cattrs"

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "pygls"

        [[distribution]]
        name = "pygls"
        version = "1.3.0"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/e9/8d/31b50ac0879464049d744a1ddf00dc6474433eb55d40fa0c8e8510591ad2/pygls-1.3.0.tar.gz", hash = "sha256:1b44ace89c9382437a717534f490eadc6fda7c0c6c16ac1eaaf5568e345e4fb8", size = 45539 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/4e/1e/643070d8f5c851958662e7e5df16d9c3a068a598a7ee7bb2eb8d95b4e5d7/pygls-1.3.0-py3-none-any.whl", hash = "sha256:d4a01414b6ed4e34e7e8fd29b77d3e88c29615df7d0bbff49bf019e15ec04b8f", size = 56031 }]

        [[distribution.dependencies]]
        name = "cattrs"

        [[distribution.dependencies]]
        name = "lsprotocol"
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
        .collect();

    // Install from the lockfile.
    // Note we need `--offline` otherwise we'll just fetch a 3.12 interpreter!
    uv_snapshot!(filters, context38.sync().arg("--offline"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning.
    Removing virtual environment at: .venv
    error: No interpreter found for Python >=3.12 in system toolchains
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
        requires-python = ">=3.11.dev0, <3.12.dev0"

        [[distribution]]
        name = "attrs"
        version = "23.2.0"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/e3/fc/f800d51204003fa8ae392c4e8278f256206e7a919b708eef054f5f4b650d/attrs-23.2.0.tar.gz", hash = "sha256:935dc3b529c262f6cf76e50877d35a4bd3c1de194fd41f47a2b7ae8f19971f30", size = 780820 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/e0/44/827b2a91a5816512fcaf3cc4ebc465ccd5d598c45cefa6703fcf4a79018f/attrs-23.2.0-py3-none-any.whl", hash = "sha256:99b87a485a5820b23b879f04c2305b44b951b502fd64be915879d77a7e8fc6f1", size = 60752 }]

        [[distribution]]
        name = "cattrs"
        version = "23.2.3"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/1e/57/c6ccd22658c4bcb3beb3f1c262e1f170cf136e913b122763d0ddd328d284/cattrs-23.2.3.tar.gz", hash = "sha256:a934090d95abaa9e911dac357e3a8699e0b4b14f8529bcc7d2b1ad9d51672b9f", size = 610215 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/b3/0d/cd4a4071c7f38385dc5ba91286723b4d1090b87815db48216212c6c6c30e/cattrs-23.2.3-py3-none-any.whl", hash = "sha256:0341994d94971052e9ee70662542699a3162ea1e0c62f7ce1b4a57f563685108", size = 57474 }]

        [[distribution.dependencies]]
        name = "attrs"

        [[distribution]]
        name = "linehaul"
        version = "1.0.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/f8/e7/74d1bd36ed26ac43bfe22e97129edaa7066f7af4bf76084b9493cd581d58/linehaul-1.0.1.tar.gz", hash = "sha256:09d71b1f6a9ab92dd8c763b3d099e4ae05c2845ee783a02d5fe731e6e2a6a997", size = 19410 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/03/73/c73588052198be06462d1a7c4653b602a109a0df0208c59e58075dc3bc73/linehaul-1.0.1-py3-none-any.whl", hash = "sha256:d19ca669008dad910868dfae7f904dfc5362583729bda344799cf7ea2ad5ef12", size = 27848 }]

        [[distribution.dependencies]]
        name = "cattrs"

        [[distribution.dependencies]]
        name = "packaging"

        [[distribution.dependencies]]
        name = "pyparsing"

        [[distribution]]
        name = "packaging"
        version = "24.0"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488 }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "linehaul"

        [[distribution]]
        name = "pyparsing"
        version = "3.1.2"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/46/3a/31fd28064d016a2182584d579e033ec95b809d8e220e74c4af6f0f2e8842/pyparsing-3.1.2.tar.gz", hash = "sha256:a1bac0ce561155ecc3ed78ca94d3c9378656ad4c94c1270de543f621420f94ad", size = 889571 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/9d/ea/6d76df31432a0e6fdf81681a895f009a4bb47b3c39036db3e1b528191d52/pyparsing-3.1.2-py3-none-any.whl", hash = "sha256:f9db75911801ed778fe61bb643079ff86601aca99fcae6345aa67292038fb742", size = 103245 }]
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
        requires-python = ">=3.11b1"

        [[distribution]]
        name = "attrs"
        version = "23.2.0"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/e3/fc/f800d51204003fa8ae392c4e8278f256206e7a919b708eef054f5f4b650d/attrs-23.2.0.tar.gz", hash = "sha256:935dc3b529c262f6cf76e50877d35a4bd3c1de194fd41f47a2b7ae8f19971f30", size = 780820 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/e0/44/827b2a91a5816512fcaf3cc4ebc465ccd5d598c45cefa6703fcf4a79018f/attrs-23.2.0-py3-none-any.whl", hash = "sha256:99b87a485a5820b23b879f04c2305b44b951b502fd64be915879d77a7e8fc6f1", size = 60752 }]

        [[distribution]]
        name = "cattrs"
        version = "23.2.3"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/1e/57/c6ccd22658c4bcb3beb3f1c262e1f170cf136e913b122763d0ddd328d284/cattrs-23.2.3.tar.gz", hash = "sha256:a934090d95abaa9e911dac357e3a8699e0b4b14f8529bcc7d2b1ad9d51672b9f", size = 610215 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/b3/0d/cd4a4071c7f38385dc5ba91286723b4d1090b87815db48216212c6c6c30e/cattrs-23.2.3-py3-none-any.whl", hash = "sha256:0341994d94971052e9ee70662542699a3162ea1e0c62f7ce1b4a57f563685108", size = 57474 }]

        [[distribution.dependencies]]
        name = "attrs"

        [[distribution]]
        name = "linehaul"
        version = "1.0.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/f8/e7/74d1bd36ed26ac43bfe22e97129edaa7066f7af4bf76084b9493cd581d58/linehaul-1.0.1.tar.gz", hash = "sha256:09d71b1f6a9ab92dd8c763b3d099e4ae05c2845ee783a02d5fe731e6e2a6a997", size = 19410 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/03/73/c73588052198be06462d1a7c4653b602a109a0df0208c59e58075dc3bc73/linehaul-1.0.1-py3-none-any.whl", hash = "sha256:d19ca669008dad910868dfae7f904dfc5362583729bda344799cf7ea2ad5ef12", size = 27848 }]

        [[distribution.dependencies]]
        name = "cattrs"

        [[distribution.dependencies]]
        name = "packaging"

        [[distribution.dependencies]]
        name = "pyparsing"

        [[distribution]]
        name = "packaging"
        version = "24.0"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488 }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "linehaul"

        [[distribution]]
        name = "pyparsing"
        version = "3.1.2"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/46/3a/31fd28064d016a2182584d579e033ec95b809d8e220e74c4af6f0f2e8842/pyparsing-3.1.2.tar.gz", hash = "sha256:a1bac0ce561155ecc3ed78ca94d3c9378656ad4c94c1270de543f621420f94ad", size = 889571 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/9d/ea/6d76df31432a0e6fdf81681a895f009a4bb47b3c39036db3e1b528191d52/pyparsing-3.1.2-py3-none-any.whl", hash = "sha256:f9db75911801ed778fe61bb643079ff86601aca99fcae6345aa67292038fb742", size = 103245 }]
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
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/23/a2/97899f6bd0e873fed3a7e67ae8d3a08b21799430fb4da15cfedf10d6e2c2/iniconfig-1.1.1.tar.gz", hash = "sha256:bc3af051d7d14b2ee5ef9969666def0cd1a000e121eaea580d4a313df4b37f32", size = 8104 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl", hash = "sha256:011e24c64b7f47f6ebd835bb12a743f2fbe9a26d4cecaa7f53bc4f35ee9da8b3", size = 4990 }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "iniconfig"
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
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "iniconfig"

        [distribution.dev-dependencies]

        [[distribution.dev-dependencies.dev]]
        name = "typing-extensions"

        [[distribution]]
        name = "typing-extensions"
        version = "4.12.2"
        source = "direct+https://files.pythonhosted.org/packages/26/9f/ad63fc0248c5379346306f8668cda6e2e2e9c95e01216d2b8ffd9ff037d0/typing_extensions-4.12.2-py3-none-any.whl"
        wheels = [{ url = "https://files.pythonhosted.org/packages/26/9f/ad63fc0248c5379346306f8668cda6e2e2e9c95e01216d2b8ffd9ff037d0/typing_extensions-4.12.2-py3-none-any.whl", hash = "sha256:04e5ca0351e0f3f85c6853954072df659d0d13fac324d0072316b67d7794700d" }]
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
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "iniconfig"
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
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "iniconfig"
        marker = "implementation_name == 'cpython'"
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
        source = "editable+."

        [[distribution.dependencies]]
        name = "b"

        [[distribution.dependencies]]
        name = "c"

        [[distribution]]
        name = "b"
        version = "0.1.0"
        source = "directory+b"

        [[distribution]]
        name = "c"
        version = "0.1.0"
        source = "directory+[TEMP_DIR]/c"
        "###
        );
    });

    Ok(())
}

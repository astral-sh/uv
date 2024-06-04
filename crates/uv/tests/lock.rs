#![cfg(all(feature = "python", feature = "pypi"))]

use anyhow::Result;
use assert_fs::prelude::*;
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
        dependencies = ["anyio==3.7.0"]
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

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1

        [[distribution]]
        name = "anyio"
        version = "3.7.0"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/c6/b3/fefbf7e78ab3b805dec67d698dc18dd505af7a18a8dd08868c9b4fa736b5/anyio-3.7.0.tar.gz", hash = "sha256:275d9973793619a5374e1c89a4f4ad3f4b0a5510a2b5b939444bee8f4c4d37ce", size = 142737 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/68/fe/7ce1926952c8a403b35029e194555558514b365ad77d75125f521a2bec62/anyio-3.7.0-py3-none-any.whl", hash = "sha256:eddca883c4175f14df8aedce21054bfca3adb70ffe76a9f607aef9d7fa2ea7f0", size = 80873 }]

        [[distribution.dependencies]]
        name = "exceptiongroup"
        version = "1.2.0"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "idna"
        version = "3.6"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "typing-extensions"
        version = "4.10.0"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "exceptiongroup"
        version = "1.2.0"
        source = "registry+https://pypi.org/simple"
        marker = "python_version < '3.11'"
        sdist = { url = "https://files.pythonhosted.org/packages/8e/1c/beef724eaf5b01bb44b6338c8c3494eff7cab376fab4904cfbbc3585dc79/exceptiongroup-1.2.0.tar.gz", hash = "sha256:91f5c769735f051a4290d52edd0858999b57e5876e9f85937691bd4c9fa3ed68", size = 26264 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/b8/9a/5028fd52db10e600f1c4674441b968cf2ea4959085bfb5b99fb1250e5f68/exceptiongroup-1.2.0-py3-none-any.whl", hash = "sha256:4bfd3996ac73b41e9b9628b04e079f193850720ea5945fc96a08633c66912f14", size = 16210 }]

        [[distribution]]
        name = "idna"
        version = "3.6"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "anyio"
        version = "3.7.0"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 }]

        [[distribution]]
        name = "typing-extensions"
        version = "4.10.0"
        source = "registry+https://pypi.org/simple"
        marker = "python_version < '3.8'"
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 }]
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
    Downloaded 4 packages in [TIME]
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
        dependencies = ["source-distribution==0.0.1"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock_without_exclude_newer(), @r###"
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

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "source-distribution"
        version = "0.0.1"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "source-distribution"
        version = "0.0.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz", hash = "sha256:1f83ed7498336c7f2ab9b002cf22583d91115ebc624053dc4eb3a45694490106", size = 2157 }
        "###
        );
    });

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

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = "git+https://github.com/astral-test/uv-public-pypackage?rev=0.0.1#0dacfd662c64cb4ceb16e6cf65a157a8b715b979"

        [[distribution]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = "git+https://github.com/astral-test/uv-public-pypackage?rev=0.0.1#0dacfd662c64cb4ceb16e6cf65a157a8b715b979"
        sdist = { url = "https://github.com/astral-test/uv-public-pypackage?rev=0.0.1#0dacfd662c64cb4ceb16e6cf65a157a8b715b979" }
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
    Downloaded 2 packages in [TIME]
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
        dependencies = ["anyio @ https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl"]
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

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1

        [[distribution]]
        name = "anyio"
        version = "4.3.0"
        source = "direct+https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl"
        wheels = [{ url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8" }]

        [[distribution.dependencies]]
        name = "exceptiongroup"
        version = "1.2.0"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "idna"
        version = "3.6"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "typing-extensions"
        version = "4.10.0"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "exceptiongroup"
        version = "1.2.0"
        source = "registry+https://pypi.org/simple"
        marker = "python_version < '3.11'"
        sdist = { url = "https://files.pythonhosted.org/packages/8e/1c/beef724eaf5b01bb44b6338c8c3494eff7cab376fab4904cfbbc3585dc79/exceptiongroup-1.2.0.tar.gz", hash = "sha256:91f5c769735f051a4290d52edd0858999b57e5876e9f85937691bd4c9fa3ed68", size = 26264 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/b8/9a/5028fd52db10e600f1c4674441b968cf2ea4959085bfb5b99fb1250e5f68/exceptiongroup-1.2.0-py3-none-any.whl", hash = "sha256:4bfd3996ac73b41e9b9628b04e079f193850720ea5945fc96a08633c66912f14", size = 16210 }]

        [[distribution]]
        name = "idna"
        version = "3.6"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "anyio"
        version = "4.3.0"
        source = "direct+https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl"

        [[distribution]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 }]

        [[distribution]]
        name = "typing-extensions"
        version = "4.10.0"
        source = "registry+https://pypi.org/simple"
        marker = "python_version < '3.11'"
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 }]
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
    Downloaded 3 packages in [TIME]
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
        dependencies = ["anyio @ https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz"]
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

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1

        [[distribution]]
        name = "anyio"
        version = "4.3.0"
        source = "direct+https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz"
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6" }

        [[distribution.dependencies]]
        name = "exceptiongroup"
        version = "1.2.0"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "idna"
        version = "3.6"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "typing-extensions"
        version = "4.10.0"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "exceptiongroup"
        version = "1.2.0"
        source = "registry+https://pypi.org/simple"
        marker = "python_version < '3.11'"
        sdist = { url = "https://files.pythonhosted.org/packages/8e/1c/beef724eaf5b01bb44b6338c8c3494eff7cab376fab4904cfbbc3585dc79/exceptiongroup-1.2.0.tar.gz", hash = "sha256:91f5c769735f051a4290d52edd0858999b57e5876e9f85937691bd4c9fa3ed68", size = 26264 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/b8/9a/5028fd52db10e600f1c4674441b968cf2ea4959085bfb5b99fb1250e5f68/exceptiongroup-1.2.0-py3-none-any.whl", hash = "sha256:4bfd3996ac73b41e9b9628b04e079f193850720ea5945fc96a08633c66912f14", size = 16210 }]

        [[distribution]]
        name = "idna"
        version = "3.6"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "anyio"
        version = "4.3.0"
        source = "direct+https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz"

        [[distribution]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 }]

        [[distribution]]
        name = "typing-extensions"
        version = "4.10.0"
        source = "registry+https://pypi.org/simple"
        marker = "python_version < '3.11'"
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 }]
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
    Downloaded 4 packages in [TIME]
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
fn lock_extra() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
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
    Resolved 7 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1

        [[distribution]]
        name = "anyio"
        version = "3.7.0"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/c6/b3/fefbf7e78ab3b805dec67d698dc18dd505af7a18a8dd08868c9b4fa736b5/anyio-3.7.0.tar.gz", hash = "sha256:275d9973793619a5374e1c89a4f4ad3f4b0a5510a2b5b939444bee8f4c4d37ce", size = 142737 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/68/fe/7ce1926952c8a403b35029e194555558514b365ad77d75125f521a2bec62/anyio-3.7.0-py3-none-any.whl", hash = "sha256:eddca883c4175f14df8aedce21054bfca3adb70ffe76a9f607aef9d7fa2ea7f0", size = 80873 }]

        [[distribution.dependencies]]
        name = "exceptiongroup"
        version = "1.2.0"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "idna"
        version = "3.6"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "typing-extensions"
        version = "4.10.0"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "exceptiongroup"
        version = "1.2.0"
        source = "registry+https://pypi.org/simple"
        marker = "python_version < '3.11'"
        sdist = { url = "https://files.pythonhosted.org/packages/8e/1c/beef724eaf5b01bb44b6338c8c3494eff7cab376fab4904cfbbc3585dc79/exceptiongroup-1.2.0.tar.gz", hash = "sha256:91f5c769735f051a4290d52edd0858999b57e5876e9f85937691bd4c9fa3ed68", size = 26264 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/b8/9a/5028fd52db10e600f1c4674441b968cf2ea4959085bfb5b99fb1250e5f68/exceptiongroup-1.2.0-py3-none-any.whl", hash = "sha256:4bfd3996ac73b41e9b9628b04e079f193850720ea5945fc96a08633c66912f14", size = 16210 }]

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
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "anyio"
        version = "3.7.0"
        source = "registry+https://pypi.org/simple"

        [distribution.optional-dependencies]

        [[distribution.optional-dependencies.test]]
        name = "iniconfig"
        version = "2.0.0"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 }]

        [[distribution]]
        name = "typing-extensions"
        version = "4.10.0"
        source = "registry+https://pypi.org/simple"
        marker = "python_version < '3.8'"
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926 }]
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
    Downloaded 4 packages in [TIME]
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
    Downloaded 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
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

        [[distribution]]
        name = "iniconfig"
        version = "1.1.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/23/a2/97899f6bd0e873fed3a7e67ae8d3a08b21799430fb4da15cfedf10d6e2c2/iniconfig-1.1.1.tar.gz", hash = "sha256:bc3af051d7d14b2ee5ef9969666def0cd1a000e121eaea580d4a313df4b37f32", size = 8104 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl", hash = "sha256:011e24c64b7f47f6ebd835bb12a743f2fbe9a26d4cecaa7f53bc4f35ee9da8b3", size = 4990 }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "iniconfig"
        version = "1.1.1"
        source = "registry+https://pypi.org/simple"
        "###
        );
    });

    // Modify the `pyproject.toml` to loosen the requirement.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
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

        [[distribution]]
        name = "iniconfig"
        version = "1.1.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/23/a2/97899f6bd0e873fed3a7e67ae8d3a08b21799430fb4da15cfedf10d6e2c2/iniconfig-1.1.1.tar.gz", hash = "sha256:bc3af051d7d14b2ee5ef9969666def0cd1a000e121eaea580d4a313df4b37f32", size = 8104 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl", hash = "sha256:011e24c64b7f47f6ebd835bb12a743f2fbe9a26d4cecaa7f53bc4f35ee9da8b3", size = 4990 }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "iniconfig"
        version = "1.1.1"
        source = "registry+https://pypi.org/simple"
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

        [[distribution]]
        name = "iniconfig"
        version = "2.0.0"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "iniconfig"
        version = "2.0.0"
        source = "registry+https://pypi.org/simple"
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

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = "git+https://github.com/astral-test/uv-public-pypackage?rev=0dacfd662c64cb4ceb16e6cf65a157a8b715b979#0dacfd662c64cb4ceb16e6cf65a157a8b715b979"

        [[distribution]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = "git+https://github.com/astral-test/uv-public-pypackage?rev=0dacfd662c64cb4ceb16e6cf65a157a8b715b979#0dacfd662c64cb4ceb16e6cf65a157a8b715b979"
        sdist = { url = "https://github.com/astral-test/uv-public-pypackage?rev=0dacfd662c64cb4ceb16e6cf65a157a8b715b979#0dacfd662c64cb4ceb16e6cf65a157a8b715b979" }
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

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = "git+https://github.com/astral-test/uv-public-pypackage?rev=main#0dacfd662c64cb4ceb16e6cf65a157a8b715b979"

        [[distribution]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = "git+https://github.com/astral-test/uv-public-pypackage?rev=main#0dacfd662c64cb4ceb16e6cf65a157a8b715b979"
        sdist = { url = "https://github.com/astral-test/uv-public-pypackage?rev=main#0dacfd662c64cb4ceb16e6cf65a157a8b715b979" }
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

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = "git+https://github.com/astral-test/uv-public-pypackage?rev=main#b270df1a2fb5d012294e9aaf05e7e0bab1e6a389"

        [[distribution]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = "git+https://github.com/astral-test/uv-public-pypackage?rev=main#b270df1a2fb5d012294e9aaf05e7e0bab1e6a389"
        sdist = { url = "https://github.com/astral-test/uv-public-pypackage?rev=main#b270df1a2fb5d012294e9aaf05e7e0bab1e6a389" }
        "###
        );
    });

    Ok(())
}

/// Lock a requirement from PyPI, respecting the `Requires-Python` metadata.
#[test]
fn lock_requires_python() -> Result<()> {
    let context = TestContext::new("3.12");

    // Require >=3.7, which is incompatible with newer versions of `pygls`.
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
          And because only project==0.1.0 is available and project depends on project, we can conclude that the requirements are unsatisfiable.

          hint: The `Requires-Python` requirement (>=3.7) defined in your `pyproject.toml` includes Python versions that are not supported by your dependencies (e.g., pygls>=1.1.0,<=1.2.1 only supports >=3.7.9, <4). Consider using a more restrictive `Requires-Python` requirement (like >=3.7.9, <4).
    "###);

    // Require >=3.7, and allow locking to a version of `pygls` that is compatible.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
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
    Resolved 6 packages in [TIME]
    "###);

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1

        [[distribution]]
        name = "dataclasses"
        version = "0.6"
        source = "registry+https://pypi.org/simple"
        marker = "python_version < '3.7'"
        sdist = { url = "https://files.pythonhosted.org/packages/59/e4/2f921edfdf1493bdc07b914cbea43bc334996df4841a34523baf73d1fb4f/dataclasses-0.6.tar.gz", hash = "sha256:6988bd2b895eef432d562370bb707d540f32f7360ab13da45340101bc2307d84", size = 36819 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/26/2f/1095cdc2868052dd1e64520f7c0d5c8c550ad297e944e641dbf1ffbb9a5d/dataclasses-0.6-py3-none-any.whl", hash = "sha256:454a69d788c7fda44efd71e259be79577822f5e3f53f029a22d08004e951dc9f", size = 14757 }]

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "pygls"
        version = "0.11.3"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "pydantic"
        version = "1.8.2"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/b9/d2/12a808613937a6b98cd50d6467352f01322dc0d8ca9fb5b94441625d6684/pydantic-1.8.2.tar.gz", hash = "sha256:26464e57ccaafe72b7ad156fdaa4e9b9ef051f69e175dbbb463283000c05ab7b", size = 263751 }
        wheels = [
        	{ url = "https://files.pythonhosted.org/packages/b8/87/7149712e2f37570498eb8a86aa2c6e95109c4dd217d03d7045fa89193eb0/pydantic-1.8.2-cp36-cp36m-macosx_10_9_x86_64.whl", hash = "sha256:05ddfd37c1720c392f4e0d43c484217b7521558302e7069ce8d318438d297739", size = 2582318 },
        	{ url = "https://files.pythonhosted.org/packages/e6/ca/b4d6cead9a6abaade586ce735646dfb5aef08ab03bfb07246af24867b5a5/pydantic-1.8.2-cp36-cp36m-manylinux1_i686.whl", hash = "sha256:a7c6002203fe2c5a1b5cbb141bb85060cbff88c2d78eccbc72d97eb7022c43e4", size = 9718327 },
        	{ url = "https://files.pythonhosted.org/packages/d3/3a/b86f7a34d2edf22ca2682649bf381b88011597f136fc4a28c5d3d38743bf/pydantic-1.8.2-cp36-cp36m-manylinux2014_i686.whl", hash = "sha256:589eb6cd6361e8ac341db97602eb7f354551482368a37f4fd086c0733548308e", size = 9718331 },
        	{ url = "https://files.pythonhosted.org/packages/2b/7c/7d0b3f2d7959b7193018896db236ded165f9bca1bb75f46f4c32fa6f4f9d/pydantic-1.8.2-cp36-cp36m-manylinux2014_x86_64.whl", hash = "sha256:10e5622224245941efc193ad1d159887872776df7a8fd592ed746aa25d071840", size = 10156056 },
        	{ url = "https://files.pythonhosted.org/packages/38/26/36c97b707300787e8d51b607fc6e94c334f473fcc7519e92e2ec4234b006/pydantic-1.8.2-cp36-cp36m-win_amd64.whl", hash = "sha256:99a9fc39470010c45c161a1dc584997f1feb13f689ecf645f59bb4ba623e586b", size = 1887510 },
        	{ url = "https://files.pythonhosted.org/packages/12/b7/825bf1578e5bd4e70813f40f8e10c11c7ddcf0e0a59faefa79c65b37a139/pydantic-1.8.2-cp37-cp37m-macosx_10_9_x86_64.whl", hash = "sha256:a83db7205f60c6a86f2c44a61791d993dff4b73135df1973ecd9eed5ea0bda20", size = 2588937 },
        	{ url = "https://files.pythonhosted.org/packages/fb/50/139033721aa3196f07e67138266fb414de0bb29b43957d39c13a743f11cc/pydantic-1.8.2-cp37-cp37m-manylinux1_i686.whl", hash = "sha256:41b542c0b3c42dc17da70554bc6f38cbc30d7066d2c2815a94499b5684582ecb", size = 9669437 },
        	{ url = "https://files.pythonhosted.org/packages/45/22/87a4fe7ed5dd82d8058734dd2b6d15ccaa4a1703ca10618c87f936e1209b/pydantic-1.8.2-cp37-cp37m-manylinux2014_i686.whl", hash = "sha256:ea5cb40a3b23b3265f6325727ddfc45141b08ed665458be8c6285e7b85bd73a1", size = 9669440 },
        	{ url = "https://files.pythonhosted.org/packages/9f/f2/2d5425efe57f6c4e06cbe5e587c1fd16929dcf0eb90bd4d3d1e1c97d1151/pydantic-1.8.2-cp37-cp37m-manylinux2014_x86_64.whl", hash = "sha256:18b5ea242dd3e62dbf89b2b0ec9ba6c7b5abaf6af85b95a97b00279f65845a23", size = 10100738 },
        	{ url = "https://files.pythonhosted.org/packages/84/6a/3b9902f79b81b4f67b6e7497f3d9c9f1e6bd7a7f4e93ccd6bc0aa8f81282/pydantic-1.8.2-cp37-cp37m-win_amd64.whl", hash = "sha256:234a6c19f1c14e25e362cb05c68afb7f183eb931dd3cd4605eafff055ebbf287", size = 1887369 },
        	{ url = "https://files.pythonhosted.org/packages/69/56/46fdbd9165ab0e29408afc2940d045397677a9d0b06d7bd15a781edd7da0/pydantic-1.8.2-cp38-cp38-macosx_10_9_x86_64.whl", hash = "sha256:021ea0e4133e8c824775a0cfe098677acf6fa5a3cbf9206a376eed3fc09302cd", size = 2640055 },
        	{ url = "https://files.pythonhosted.org/packages/8f/27/11476d8d9fc95e511befc116849333421c199d01b235ede09a20870c64b2/pydantic-1.8.2-cp38-cp38-manylinux1_i686.whl", hash = "sha256:e710876437bc07bd414ff453ac8ec63d219e7690128d925c6e82889d674bb505", size = 12634763 },
        	{ url = "https://files.pythonhosted.org/packages/2a/50/37bfbc8facf3f98f7439df08b8bcbeb495760d799a5f67ac84d855455eb7/pydantic-1.8.2-cp38-cp38-manylinux2014_i686.whl", hash = "sha256:ac8eed4ca3bd3aadc58a13c2aa93cd8a884bcf21cb019f8cfecaae3b6ce3746e", size = 12634765 },
        	{ url = "https://files.pythonhosted.org/packages/34/f8/438aa7b258607ea875ca71b9f549748e75eca0f4f42a4447112c7074cca3/pydantic-1.8.2-cp38-cp38-manylinux2014_x86_64.whl", hash = "sha256:4a03cbbe743e9c7247ceae6f0d8898f7a64bb65800a45cbdc52d65e370570820", size = 13702235 },
        	{ url = "https://files.pythonhosted.org/packages/51/68/6579cb896863715b6a5c63e4983b1c0ab7693685a7c2ded469ef37eb3539/pydantic-1.8.2-cp38-cp38-win_amd64.whl", hash = "sha256:8621559dcf5afacf0069ed194278f35c255dc1a1385c28b32dd6c110fd6531b3", size = 1984584 },
        	{ url = "https://files.pythonhosted.org/packages/98/5e/30b8c83596af6f28f8e8fd9c136ff867ae1075a11baabbc87aaf274fb98f/pydantic-1.8.2-cp39-cp39-macosx_10_9_x86_64.whl", hash = "sha256:8b223557f9510cf0bfd8b01316bf6dd281cf41826607eada99662f5e4963f316", size = 2697981 },
        	{ url = "https://files.pythonhosted.org/packages/0c/f1/003464c232cbd415f48074e349a27ee1a746641ce2594da9ad8f656b0238/pydantic-1.8.2-cp39-cp39-manylinux1_i686.whl", hash = "sha256:244ad78eeb388a43b0c927e74d3af78008e944074b7d0f4f696ddd5b2af43c62", size = 10839963 },
        	{ url = "https://files.pythonhosted.org/packages/16/c9/ac98688c9083c54fd5cdbb3179f33c4ebcdc081bc94441ae41c8fb35782f/pydantic-1.8.2-cp39-cp39-manylinux2014_i686.whl", hash = "sha256:05ef5246a7ffd2ce12a619cbb29f3307b7c4509307b1b49f456657b43529dc6f", size = 10839966 },
        	{ url = "https://files.pythonhosted.org/packages/d8/a3/b03397aca3de5aa7e1353c2bd2c9753c7a7ce5e001b3a5b2da98c6bdde13/pydantic-1.8.2-cp39-cp39-manylinux2014_x86_64.whl", hash = "sha256:54cd5121383f4a461ff7644c7ca20c0419d58052db70d8791eacbbe31528916b", size = 11343585 },
        	{ url = "https://files.pythonhosted.org/packages/5a/8c/5eb271ba26497e9bff1a1aa6d3d35a1f1c7e73f28012ad7c0e93d376ffcb/pydantic-1.8.2-cp39-cp39-win_amd64.whl", hash = "sha256:4be75bebf676a5f0f87937c6ddb061fa39cbea067240d98e298508c1bda6f3f3", size = 1949575 },
        	{ url = "https://files.pythonhosted.org/packages/ff/74/54e030641601112309f6d2af620774e9080f99c7a15742fc6a0b170c4076/pydantic-1.8.2-py3-none-any.whl", hash = "sha256:fec866a0b59f372b7e776f2d7308511784dace622e0992a0b59ea3ccee0ae833", size = 126035 }
        ]

        [[distribution.dependencies]]
        name = "dataclasses"
        version = "0.6"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "typing-extensions"
        version = "4.7.1"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "pygls"
        version = "0.11.3"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/f9/de/760ead0c7169391720fbb8fbc2c64ca32af8af559a10377f06739408c2ce/pygls-0.11.3.tar.gz", hash = "sha256:4d86fc854e6d6613cd42bf7511e9c6aac947fc8d62ff973a705570b036d969f2", size = 139426 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/fc/df/3ec0e1a9e3945545339cf95b8fe5445150d37addf26f49734fa481f5eedc/pygls-0.11.3-py3-none-any.whl", hash = "sha256:5c925b182f2b0aa38d0ce83a9829ca5aed8eb9c7079cffc5bddff2da1033b58f", size = 86726 }]

        [[distribution.dependencies]]
        name = "pydantic"
        version = "1.8.2"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "typeguard"
        version = "2.13.3"
        source = "registry+https://pypi.org/simple"

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
        "###
        );
    });

    // Remove the lockfile.
    fs_err::remove_file(context.temp_dir.join("uv.lock"))?;

    // Bump the Python requirement, which should allow a newer version of `pygls`.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.7.9,<4"
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

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1

        [[distribution]]
        name = "attrs"
        version = "23.2.0"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/e3/fc/f800d51204003fa8ae392c4e8278f256206e7a919b708eef054f5f4b650d/attrs-23.2.0.tar.gz", hash = "sha256:935dc3b529c262f6cf76e50877d35a4bd3c1de194fd41f47a2b7ae8f19971f30", size = 780820 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/e0/44/827b2a91a5816512fcaf3cc4ebc465ccd5d598c45cefa6703fcf4a79018f/attrs-23.2.0-py3-none-any.whl", hash = "sha256:99b87a485a5820b23b879f04c2305b44b951b502fd64be915879d77a7e8fc6f1", size = 60752 }]

        [[distribution.dependencies]]
        name = "importlib-metadata"
        version = "6.7.0"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "cattrs"
        version = "23.1.2"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/68/d4/27f9fd840e74d51b6d6a024d39ff495b56ffde71d28eb82758b7b85d0617/cattrs-23.1.2.tar.gz", hash = "sha256:db1c821b8c537382b2c7c66678c3790091ca0275ac486c76f3c8f3920e83c657", size = 39998 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/3a/ba/05df14efaa0624fac6b1510e87f5ce446208d2f6ce50270a89b6268aebfe/cattrs-23.1.2-py3-none-any.whl", hash = "sha256:b2bb14311ac17bed0d58785e5a60f022e5431aca3932e3fc5cc8ed8639de50a4", size = 50845 }]

        [[distribution.dependencies]]
        name = "attrs"
        version = "23.2.0"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "exceptiongroup"
        version = "1.2.0"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "typing-extensions"
        version = "4.7.1"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "exceptiongroup"
        version = "1.2.0"
        source = "registry+https://pypi.org/simple"
        marker = "python_version < '3.11'"
        sdist = { url = "https://files.pythonhosted.org/packages/8e/1c/beef724eaf5b01bb44b6338c8c3494eff7cab376fab4904cfbbc3585dc79/exceptiongroup-1.2.0.tar.gz", hash = "sha256:91f5c769735f051a4290d52edd0858999b57e5876e9f85937691bd4c9fa3ed68", size = 26264 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/b8/9a/5028fd52db10e600f1c4674441b968cf2ea4959085bfb5b99fb1250e5f68/exceptiongroup-1.2.0-py3-none-any.whl", hash = "sha256:4bfd3996ac73b41e9b9628b04e079f193850720ea5945fc96a08633c66912f14", size = 16210 }]

        [[distribution]]
        name = "importlib-metadata"
        version = "6.7.0"
        source = "registry+https://pypi.org/simple"
        marker = "python_version < '3.8'"
        sdist = { url = "https://files.pythonhosted.org/packages/a3/82/f6e29c8d5c098b6be61460371c2c5591f4a335923639edec43b3830650a4/importlib_metadata-6.7.0.tar.gz", hash = "sha256:1aaf550d4f73e5d6783e7acb77aec43d49da8017410afae93822cc9cca98c4d4", size = 53569 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/ff/94/64287b38c7de4c90683630338cf28f129decbba0a44f0c6db35a873c73c4/importlib_metadata-6.7.0-py3-none-any.whl", hash = "sha256:cb52082e659e97afc5dac71e79de97d8681de3aa07ff18578330904a9d18e5b5", size = 22934 }]

        [[distribution.dependencies]]
        name = "typing-extensions"
        version = "4.7.1"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "zipp"
        version = "3.15.0"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "lsprotocol"
        version = "2023.0.0"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/3e/fe/f7671a4fb28606ff1663bba60aff6af21b1e43a977c74c33db13cb83680f/lsprotocol-2023.0.0.tar.gz", hash = "sha256:c9d92e12a3f4ed9317d3068226592860aab5357d93cf5b2451dc244eee8f35f2", size = 69399 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/2d/5b/f18eb1823a4cee9bed70cdcc25eed5a75845367c42e63a79010a7c34f8a7/lsprotocol-2023.0.0-py3-none-any.whl", hash = "sha256:e85fc87ee26c816adca9eb497bb3db1a7c79c477a11563626e712eaccf926a05", size = 70789 }]

        [[distribution.dependencies]]
        name = "attrs"
        version = "23.2.0"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "cattrs"
        version = "23.1.2"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"
        sdist = { url = "file://[TEMP_DIR]/" }

        [[distribution.dependencies]]
        name = "pygls"
        version = "1.2.1"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "pygls"
        version = "1.2.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/e6/94/534c11ba5475df09542e48d751a66e0448d52bbbb92cbef5541deef7760d/pygls-1.2.1.tar.gz", hash = "sha256:04f9b9c115b622dcc346fb390289066565343d60245a424eca77cb429b911ed8", size = 45274 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/36/31/3799444d3f072ffca1a35eb02a48f964384cc13f001125e87d9f0748687b/pygls-1.2.1-py3-none-any.whl", hash = "sha256:7dcfcf12b6f15beb606afa46de2ed348b65a279c340ef2242a9a35c22eeafe94", size = 55983 }]

        [[distribution.dependencies]]
        name = "lsprotocol"
        version = "2023.0.0"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "typing-extensions"
        version = "4.7.1"
        source = "registry+https://pypi.org/simple"
        marker = "python_version < '3.8' or python_version < '3.11'"
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

    Ok(())
}

#![cfg(all(feature = "python", feature = "pypi"))]

use anyhow::Result;
use assert_fs::prelude::*;
use indoc::indoc;
use insta::assert_snapshot;

use common::{uv_snapshot, TestContext};

mod common;

/// Add a PyPI requirement.
#[test]
fn add_registry() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        # ...
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.add(&["anyio==3.7.0"]), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv add` is experimental and may change without warning
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.1
    "###);

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r###"
        [project]
        name = "project"
        version = "0.1.0"
        # ...
        requires-python = ">=3.12"
        dependencies = [
            "anyio==3.7.0",
        ]
        "###
        );
    });

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        exclude-newer = "2024-03-25 00:00:00 UTC"

        [[distribution]]
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
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning
    Audited 4 packages in [TIME]
    "###);

    Ok(())
}

/// Add a Git requirement.
#[test]
fn add_git() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]
    "#})?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 4 packages in [TIME]
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.1
    "###);

    // Adding with an ambiguous Git reference will fail.
    uv_snapshot!(context.filters(), context.add(&["uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@0.0.1"]).arg("--preview"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot resolve Git reference `0.0.1` for requirement `uv-public-pypackage`. Specify the reference with one of `--tag`, `--branch`, or `--rev`, or use the `--raw-sources` flag.
    "###);

    uv_snapshot!(context.filters(), context.add(&["uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage"]).arg("--tag=0.0.1").arg("--preview"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 2 packages in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 2 packages in [TIME]
     - project==0.1.0 (from file://[TEMP_DIR]/)
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@0dacfd662c64cb4ceb16e6cf65a157a8b715b979?tag=0.0.1#0dacfd662c64cb4ceb16e6cf65a157a8b715b979)
    "###);

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r###"
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
        "###
        );
    });

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        exclude-newer = "2024-03-25 00:00:00 UTC"

        [[distribution]]
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
            { name = "uv-public-pypackage" },
        ]

        [[distribution]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]

        [[distribution]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = { git = "https://github.com/astral-test/uv-public-pypackage?tag=0.0.1#0dacfd662c64cb4ceb16e6cf65a157a8b715b979" }
        "###
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning
    Audited 5 packages in [TIME]
    "###);

    Ok(())
}

#[test]
fn add_git_error() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 1 package in [TIME]
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/)
    "###);

    // Provide a tag without a Git source.
    uv_snapshot!(context.filters(), context.add(&[]).arg("flask").arg("--tag").arg("0.0.1").arg("--preview"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `flask` did not resolve to a Git repository, but a Git reference (`--tag 0.0.1`) was provided.
    "###);

    // Provide a tag with a non-Git source.
    uv_snapshot!(context.filters(), context.add(&[]).arg("flask @ https://files.pythonhosted.org/packages/61/80/ffe1da13ad9300f87c93af113edd0638c75138c42a0994becfacac078c06/flask-3.0.3-py3-none-any.whl").arg("--branch").arg("0.0.1").arg("--preview"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: `flask` did not resolve to a Git repository, but a Git reference (`--branch 0.0.1`) was provided.
    "###);

    Ok(())
}

/// Add a Git requirement using the `--raw-sources` API.
#[test]
fn add_git_raw() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]
    "#})?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 4 packages in [TIME]
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.1
    "###);

    // Use an ambiguous tag reference, which would otherwise not resolve.
    uv_snapshot!(context.filters(), context.add(&["uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@0.0.1"]).arg("--raw-sources").arg("--preview"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 2 packages in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 2 packages in [TIME]
     - project==0.1.0 (from file://[TEMP_DIR]/)
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@0dacfd662c64cb4ceb16e6cf65a157a8b715b979?rev=0.0.1#0dacfd662c64cb4ceb16e6cf65a157a8b715b979)
    "###);

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r###"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio==3.7.0",
            "uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage@0.0.1",
        ]
        "###
        );
    });

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        exclude-newer = "2024-03-25 00:00:00 UTC"

        [[distribution]]
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
            { name = "uv-public-pypackage" },
        ]

        [[distribution]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
        ]

        [[distribution]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = { git = "https://github.com/astral-test/uv-public-pypackage?rev=0.0.1#0dacfd662c64cb4ceb16e6cf65a157a8b715b979" }
        "###
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning
    Audited 5 packages in [TIME]
    "###);

    Ok(())
}

/// `--raw-sources` should be considered conflicting with sources-specific arguments, like `--tag`.
#[test]
fn add_raw_error() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Provide a tag without a Git source.
    uv_snapshot!(context.filters(), context.add(&[]).arg("uv-public-pypackage @ git+https://github.com/astral-test/uv-public-pypackage").arg("--tag").arg("0.0.1").arg("--raw-sources").arg("--preview"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--tag <TAG>' cannot be used with '--raw-sources'

    Usage: uv add --cache-dir [CACHE_DIR] --tag <TAG> --exclude-newer <EXCLUDE_NEWER> <REQUIREMENTS>...

    For more information, try '--help'.
    "###);

    Ok(())
}

/// Add an unnamed requirement.
#[test]
fn add_unnamed() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        # ...
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.add(&["git+https://github.com/astral-test/uv-public-pypackage"]).arg("--tag=0.0.1").arg("--preview"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + uv-public-pypackage==0.1.0 (from git+https://github.com/astral-test/uv-public-pypackage@0dacfd662c64cb4ceb16e6cf65a157a8b715b979?tag=0.0.1#0dacfd662c64cb4ceb16e6cf65a157a8b715b979)
    "###);

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r###"
        [project]
        name = "project"
        version = "0.1.0"
        # ...
        requires-python = ">=3.12"
        dependencies = [
            "uv-public-pypackage",
        ]

        [tool.uv.sources]
        uv-public-pypackage = { git = "https://github.com/astral-test/uv-public-pypackage", tag = "0.0.1" }
        "###
        );
    });

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        exclude-newer = "2024-03-25 00:00:00 UTC"

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
        source = { git = "https://github.com/astral-test/uv-public-pypackage?tag=0.0.1#0dacfd662c64cb4ceb16e6cf65a157a8b715b979" }
        "###
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning
    Audited 2 packages in [TIME]
    "###);

    Ok(())
}

/// Add and remove a development dependency.
#[test]
fn add_remove_dev() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.add(&["anyio==3.7.0"]).arg("--dev"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv add` is experimental and may change without warning
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.1
    "###);

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r###"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        dev-dependencies = [
            "anyio==3.7.0",
        ]
        "###
        );
    });

    // `uv add` implies a full lock and sync, including development dependencies.
    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        exclude-newer = "2024-03-25 00:00:00 UTC"

        [[distribution]]
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

        [distribution.dev-dependencies]
        dev = [
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
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning
    Audited 4 packages in [TIME]
    "###);

    // This should fail without --dev.
    uv_snapshot!(context.filters(), context.remove(&["anyio"]), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv remove` is experimental and may change without warning
    warning: `anyio` is a development dependency; try calling `uv remove --dev`
    error: The dependency `anyio` could not be found in `dependencies`
    "###);

    // Remove the dependency.
    uv_snapshot!(context.filters(), context.remove(&["anyio"]).arg("--dev"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv remove` is experimental and may change without warning
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 4 packages in [TIME]
    Installed 1 package in [TIME]
     - anyio==3.7.0
     - idna==3.6
     - project==0.1.0 (from file://[TEMP_DIR]/)
     + project==0.1.0 (from file://[TEMP_DIR]/)
     - sniffio==1.3.1
    "###);

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r###"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        dev-dependencies = []
        "###
        );
    });

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        exclude-newer = "2024-03-25 00:00:00 UTC"

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        "###
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning
    Audited 1 package in [TIME]
    "###);

    Ok(())
}

/// Add and remove an optional dependency.
#[test]
fn add_remove_optional() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.add(&["anyio==3.7.0"]).arg("--optional=io"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv add` is experimental and may change without warning
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/)
    "###);

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r###"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.optional-dependencies]
        io = [
            "anyio==3.7.0",
        ]
        "###
        );
    });

    // `uv add` implies a full lock and sync, including development dependencies.
    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        exclude-newer = "2024-03-25 00:00:00 UTC"

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        "###
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning
    Audited 1 package in [TIME]
    "###);

    // This should fail without --optional.
    uv_snapshot!(context.filters(), context.remove(&["anyio"]), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv remove` is experimental and may change without warning
    warning: `anyio` is an optional dependency; try calling `uv remove --optional io`
    error: The dependency `anyio` could not be found in `dependencies`
    "###);

    // Remove the dependency.
    uv_snapshot!(context.filters(), context.remove(&["anyio"]).arg("--optional=io"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv remove` is experimental and may change without warning
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - project==0.1.0 (from file://[TEMP_DIR]/)
     + project==0.1.0 (from file://[TEMP_DIR]/)
    "###);

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r###"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [project.optional-dependencies]
        io = []
        "###
        );
    });

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        exclude-newer = "2024-03-25 00:00:00 UTC"

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        "###
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning
    Audited 1 package in [TIME]
    "###);

    Ok(())
}

/// Add and remove a workspace dependency.
#[test]
fn add_remove_workspace() -> Result<()> {
    let context = TestContext::new("3.12");

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
    "#})?;

    let pyproject_toml = context.temp_dir.child("child2/pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "child2"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    // Adding a workspace package with a mismatched source should error.
    let mut add_cmd =
        context.add(&["child2 @ git+https://github.com/astral-test/uv-public-pypackage"]);
    add_cmd
        .arg("--preview")
        .arg("--package")
        .arg("child1")
        .current_dir(&context.temp_dir);

    uv_snapshot!(context.filters(), add_cmd, @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Workspace dependency `child2` must refer to local directory, not a Git repository
    "###);

    // Workspace packages should be detected automatically.
    let child1 = context.temp_dir.join("child1");
    let mut add_cmd = context.add(&["child2"]);
    add_cmd
        .arg("--preview")
        .arg("--package")
        .arg("child1")
        .current_dir(&context.temp_dir);

    uv_snapshot!(context.filters(), add_cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + child1==0.1.0 (from file://[TEMP_DIR]/child1)
     + child2==0.1.0 (from file://[TEMP_DIR]/child2)
    "###);

    let pyproject_toml = fs_err::read_to_string(child1.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r###"
        [project]
        name = "child1"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "child2",
        ]

        [tool.uv.sources]
        child2 = { workspace = true }
        "###
        );
    });

    // `uv add` implies a full lock and sync, including development dependencies.
    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        exclude-newer = "2024-03-25 00:00:00 UTC"

        [[distribution]]
        name = "child1"
        version = "0.1.0"
        source = { editable = "child1" }
        dependencies = [
            { name = "child2" },
        ]

        [[distribution]]
        name = "child2"
        version = "0.1.0"
        source = { editable = "child2" }
        "###
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").current_dir(&child1), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning
    Audited 2 packages in [TIME]
    "###);

    // Remove the dependency.
    uv_snapshot!(context.filters(), context.remove(&["child2"]).current_dir(&child1), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv remove` is experimental and may change without warning
    Resolved 2 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 2 packages in [TIME]
    Installed 1 package in [TIME]
     - child1==0.1.0 (from file://[TEMP_DIR]/child1)
     + child1==0.1.0 (from file://[TEMP_DIR]/child1)
     - child2==0.1.0 (from file://[TEMP_DIR]/child2)
    "###);

    let pyproject_toml = fs_err::read_to_string(child1.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r###"
        [project]
        name = "child1"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv.sources]
        "###
        );
    });

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        exclude-newer = "2024-03-25 00:00:00 UTC"

        [[distribution]]
        name = "child1"
        version = "0.1.0"
        source = { editable = "child1" }

        [[distribution]]
        name = "child2"
        version = "0.1.0"
        source = { editable = "child2" }
        "###
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").current_dir(&child1), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning
    Audited 1 package in [TIME]
    "###);

    Ok(())
}

/// Add a workspace dependency as an editable.
#[test]
fn add_workspace_editable() -> Result<()> {
    let context = TestContext::new("3.12");

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
    "#})?;

    let pyproject_toml = context.temp_dir.child("child2/pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "child2"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    let child1 = context.temp_dir.join("child1");
    let mut add_cmd = context.add(&["child2"]);
    add_cmd
        .arg("--editable")
        .arg("--preview")
        .current_dir(&child1);

    uv_snapshot!(context.filters(), add_cmd, @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + child1==0.1.0 (from file://[TEMP_DIR]/child1)
     + child2==0.1.0 (from file://[TEMP_DIR]/child2)
    "###);

    let pyproject_toml = fs_err::read_to_string(child1.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r###"
        [project]
        name = "child1"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "child2",
        ]

        [tool.uv.sources]
        child2 = { workspace = true, editable = true }
        "###
        );
    });

    // `uv add` implies a full lock and sync, including development dependencies.
    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        exclude-newer = "2024-03-25 00:00:00 UTC"

        [[distribution]]
        name = "child1"
        version = "0.1.0"
        source = { editable = "child1" }
        dependencies = [
            { name = "child2" },
        ]

        [[distribution]]
        name = "child2"
        version = "0.1.0"
        source = { editable = "child2" }
        "###
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").current_dir(&child1), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning
    Audited 2 packages in [TIME]
    "###);

    Ok(())
}

/// Update a requirement, modifying the source and extras.
#[test]
fn update() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "requests==2.31.0"
        ]
    "#})?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 6 packages in [TIME]
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + requests==2.31.0
     + urllib3==2.2.1
    "###);

    // Enable an extra (note the version specifier should be preserved).
    uv_snapshot!(context.filters(), context.add(&["requests[security]"]), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv add` is experimental and may change without warning
    Resolved 6 packages in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - project==0.1.0 (from file://[TEMP_DIR]/)
     + project==0.1.0 (from file://[TEMP_DIR]/)
    "###);

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r###"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "requests[security]==2.31.0",
        ]
        "###
        );
    });

    // Enable extras using the CLI flag and add a marker.
    uv_snapshot!(context.filters(), context.add(&["requests; python_version > '3.7'"]).args(["--extra=use_chardet_on_py3", "--extra=socks"]), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv add` is experimental and may change without warning
    Resolved 8 packages in [TIME]
    Prepared 3 packages in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 3 packages in [TIME]
     + chardet==5.2.0
     - project==0.1.0 (from file://[TEMP_DIR]/)
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + pysocks==1.7.1
    "###);

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r###"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "requests[security,socks,use-chardet-on-py3]==2.31.0 ; python_version > '3.7'",
        ]
        "###
        );
    });

    // Change the source by specifying a version (note the extras, markers, and version should be
    // preserved).
    uv_snapshot!(context.filters(), context.add(&["requests @ git+https://github.com/psf/requests"]).arg("--tag=v2.32.3"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv add` is experimental and may change without warning
    warning: `uv.sources` is experimental and may change without warning
    Resolved 8 packages in [TIME]
    Prepared 2 packages in [TIME]
    Uninstalled 2 packages in [TIME]
    Installed 2 packages in [TIME]
     - project==0.1.0 (from file://[TEMP_DIR]/)
     + project==0.1.0 (from file://[TEMP_DIR]/)
     - requests==2.31.0
     + requests==2.32.3 (from git+https://github.com/psf/requests@0e322af87745eff34caffe4df68456ebc20d9068?tag=v2.32.3#0e322af87745eff34caffe4df68456ebc20d9068)
    "###);

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r###"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "requests[security,socks,use-chardet-on-py3]==2.31.0 ; python_version > '3.7'",
        ]

        [tool.uv.sources]
        requests = { git = "https://github.com/psf/requests", tag = "v2.32.3" }
        "###
        );
    });

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        exclude-newer = "2024-03-25 00:00:00 UTC"

        [[distribution]]
        name = "certifi"
        version = "2024.2.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/71/da/e94e26401b62acd6d91df2b52954aceb7f561743aa5ccc32152886c76c96/certifi-2024.2.2.tar.gz", hash = "sha256:0569859f95fc761b18b45ef421b1290a0f65f147e92a1e5eb3e635f9a5e4e66f", size = 164886 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ba/06/a07f096c664aeb9f01624f858c3add0a4e913d6c96257acb4fce61e7de14/certifi-2024.2.2-py3-none-any.whl", hash = "sha256:dc383c07b76109f368f6106eee2b593b04a011ea4d55f652c6ca24a754d1cdd1", size = 163774 },
        ]

        [[distribution]]
        name = "chardet"
        version = "5.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/f3/0d/f7b6ab21ec75897ed80c17d79b15951a719226b9fababf1e40ea74d69079/chardet-5.2.0.tar.gz", hash = "sha256:1b3b6ff479a8c414bc3fa2c0852995695c4a026dcd6d0633b2dd092ca39c1cf7", size = 2069618 }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/38/6f/f5fbc992a329ee4e0f288c1fe0e2ad9485ed064cac731ed2fe47dcc38cbf/chardet-5.2.0-py3-none-any.whl", hash = "sha256:e1cf59446890a00105fe7b7912492ea04b6e6f06d4b742b2c788469e34c82970", size = 199385 },
        ]

        [[distribution]]
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
            { name = "requests", extra = ["socks", "use-chardet-on-py3"] },
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
        version = "2.32.3"
        source = { git = "https://github.com/psf/requests?tag=v2.32.3#0e322af87745eff34caffe4df68456ebc20d9068" }
        dependencies = [
            { name = "certifi" },
            { name = "charset-normalizer" },
            { name = "idna" },
            { name = "urllib3" },
        ]

        [distribution.optional-dependencies]
        socks = [
            { name = "pysocks" },
        ]
        use-chardet-on-py3 = [
            { name = "chardet" },
        ]

        [[distribution]]
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

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning
    Audited 8 packages in [TIME]
    "###);

    Ok(())
}

#[test]
fn update_source_replace_url() -> Result<()> {
    let context = TestContext::new("3.12");

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
    uv_snapshot!(context.filters(), context.add(&["requests @ git+https://github.com/psf/requests"]).arg("--tag=v2.32.3"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv add` is experimental and may change without warning
    warning: `uv.sources` is experimental and may change without warning
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + requests==2.32.3 (from git+https://github.com/psf/requests@0e322af87745eff34caffe4df68456ebc20d9068?tag=v2.32.3#0e322af87745eff34caffe4df68456ebc20d9068)
     + urllib3==2.2.1
    "###);

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r###"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "requests[security]",
        ]

        [tool.uv.sources]
        requests = { git = "https://github.com/psf/requests", tag = "v2.32.3" }
        "###
        );
    });

    Ok(())
}

/// Adding a dependency does not clean the environment.
#[test]
fn add_no_clean() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "anyio == 3.7.0",
        ]
    "#})?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 4 packages in [TIME]
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.1
    "###);

    // Manually remove a dependency.
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.add(&["iniconfig==2.0.0"]), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv add` is experimental and may change without warning
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 2 packages in [TIME]
     + iniconfig==2.0.0
     - project==0.1.0 (from file://[TEMP_DIR]/)
     + project==0.1.0 (from file://[TEMP_DIR]/)
    "###);

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r###"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "iniconfig==2.0.0",
        ]
        "###
        );
    });

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        exclude-newer = "2024-03-25 00:00:00 UTC"

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

    // Install from the lockfile without cleaning the environment.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen").arg("--no-clean"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning
    Audited 2 packages in [TIME]
    "###);

    // Install from the lockfile, cleaning the environment.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning
    Uninstalled 3 packages in [TIME]
     - anyio==3.7.0
     - idna==3.6
     - sniffio==1.3.1
    "###);

    Ok(())
}

/// Remove a PyPI requirement.
#[test]
fn remove_registry() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]
    "#})?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning
    Resolved 4 packages in [TIME]
    "###);

    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + anyio==3.7.0
     + idna==3.6
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.1
    "###);

    uv_snapshot!(context.filters(), context.remove(&["anyio"]), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv remove` is experimental and may change without warning
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 4 packages in [TIME]
    Installed 1 package in [TIME]
     - anyio==3.7.0
     - idna==3.6
     - project==0.1.0 (from file://[TEMP_DIR]/)
     + project==0.1.0 (from file://[TEMP_DIR]/)
     - sniffio==1.3.1
    "###);

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r###"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []
        "###
        );
    });

    let lock = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r###"
        version = 1
        requires-python = ">=3.12"
        exclude-newer = "2024-03-25 00:00:00 UTC"

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        "###
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning
    Audited 1 package in [TIME]
    "###);

    Ok(())
}

#[test]
fn add_preserves_indentation_in_pyproject_toml() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        # ...
        requires-python = ">=3.12"
        dependencies = [
          "anyio==3.7.0"
        ]
    "#})?;

    uv_snapshot!(context.filters(), context.add(&["requests==2.31.0"]), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv add` is experimental and may change without warning
    Resolved 8 packages in [TIME]
    Prepared 8 packages in [TIME]
    Installed 8 packages in [TIME]
     + anyio==3.7.0
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + requests==2.31.0
     + sniffio==1.3.1
     + urllib3==2.2.1
    "###);

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r###"
        [project]
        name = "project"
        version = "0.1.0"
        # ...
        requires-python = ">=3.12"
        dependencies = [
          "anyio==3.7.0",
          "requests==2.31.0",
        ]
        "###
        );
    });
    Ok(())
}

#[test]
fn add_puts_default_indentation_in_pyproject_toml_if_not_observed() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        # ...
        requires-python = ">=3.12"
        dependencies = ["anyio==3.7.0"]
    "#})?;

    uv_snapshot!(context.filters(), context.add(&["requests==2.31.0"]), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv add` is experimental and may change without warning
    Resolved 8 packages in [TIME]
    Prepared 8 packages in [TIME]
    Installed 8 packages in [TIME]
     + anyio==3.7.0
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + idna==3.6
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + requests==2.31.0
     + sniffio==1.3.1
     + urllib3==2.2.1
    "###);

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r###"
        [project]
        name = "project"
        version = "0.1.0"
        # ...
        requires-python = ">=3.12"
        dependencies = [
            "anyio==3.7.0",
            "requests==2.31.0",
        ]
        "###
        );
    });
    Ok(())
}

/// Add a requirement without updating the lockfile.
#[test]
fn add_frozen() -> Result<()> {
    let context = TestContext::new("3.12");

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        # ...
        requires-python = ">=3.12"
        dependencies = []
    "#})?;

    uv_snapshot!(context.filters(), context.add(&["anyio==3.7.0"]).arg("--frozen"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv add` is experimental and may change without warning
    "###);

    let pyproject_toml = fs_err::read_to_string(context.temp_dir.join("pyproject.toml"))?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            pyproject_toml, @r###"
        [project]
        name = "project"
        version = "0.1.0"
        # ...
        requires-python = ">=3.12"
        dependencies = [
            "anyio==3.7.0",
        ]
        "###
        );
    });

    assert!(!context.temp_dir.join("uv.lock").exists());

    Ok(())
}

#[test]
fn add_reject_multiple_git_ref_flags() {
    let context = TestContext::new("3.12");

    // --tag and --branch
    uv_snapshot!(context
        .add(&[])
        .arg("foo")
        .arg("--tag")
        .arg("0.0.1")
        .arg("--branch")
        .arg("test"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--tag <TAG>' cannot be used with '--branch <BRANCH>'

    Usage: uv add --cache-dir [CACHE_DIR] --tag <TAG> --exclude-newer <EXCLUDE_NEWER> <REQUIREMENTS>...

    For more information, try '--help'.
    "###
    );

    // --tag and --rev
    uv_snapshot!(context
        .add(&[])
        .arg("foo")
        .arg("--tag")
        .arg("0.0.1")
        .arg("--rev")
        .arg("326b943"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--tag <TAG>' cannot be used with '--rev <REV>'

    Usage: uv add --cache-dir [CACHE_DIR] --tag <TAG> --exclude-newer <EXCLUDE_NEWER> <REQUIREMENTS>...

    For more information, try '--help'.
    "###
    );

    // --tag and --tag
    uv_snapshot!(context
        .add(&[])
        .arg("foo")
        .arg("--tag")
        .arg("0.0.1")
        .arg("--tag")
        .arg("0.0.2"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--tag <TAG>' cannot be used multiple times

    Usage: uv add [OPTIONS] <REQUIREMENTS>...

    For more information, try '--help'.
    "###
    );
}

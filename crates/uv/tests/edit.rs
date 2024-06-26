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
    warning: `uv add` is experimental and may change without warning.
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
    warning: `uv lock` is experimental and may change without warning.
    Resolved 4 packages in [TIME]
    "###);

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

        [[distribution.dependencies]]
        name = "uv-public-pypackage"

        [[distribution]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 }]

        [[distribution]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = "git+https://github.com/astral-test/uv-public-pypackage?tag=0.0.1#0dacfd662c64cb4ceb16e6cf65a157a8b715b979"
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
    Audited 5 packages in [TIME]
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
    warning: `uv lock` is experimental and may change without warning.
    Resolved 4 packages in [TIME]
    "###);

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

        [[distribution.dependencies]]
        name = "uv-public-pypackage"

        [[distribution]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 }]

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
    Audited 5 packages in [TIME]
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

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."

        [[distribution.dependencies]]
        name = "uv-public-pypackage"

        [[distribution]]
        name = "uv-public-pypackage"
        version = "0.1.0"
        source = "git+https://github.com/astral-test/uv-public-pypackage?tag=0.0.1#0dacfd662c64cb4ceb16e6cf65a157a8b715b979"
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
    warning: `uv add` is experimental and may change without warning.
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

        [distribution.dev-dependencies]

        [[distribution.dev-dependencies.dev]]
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
    Audited 4 packages in [TIME]
    "###);

    // This should fail without --dev.
    uv_snapshot!(context.filters(), context.remove(&["anyio"]), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    warning: `uv remove` is experimental and may change without warning.
    warning: `anyio` is a development dependency; try calling `uv remove --dev`
    error: The dependency `anyio` could not be found in `dependencies`
    "###);

    // Remove the dependency.
    uv_snapshot!(context.filters(), context.remove(&["anyio"]).arg("--dev"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv remove` is experimental and may change without warning.
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

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."
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

        [[distribution]]
        name = "child1"
        version = "0.1.0"
        source = "editable+child1"

        [[distribution.dependencies]]
        name = "child2"

        [[distribution]]
        name = "child2"
        version = "0.1.0"
        source = "editable+child2"
        "###
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().current_dir(&child1), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning.
    Audited 2 packages in [TIME]
    "###);

    // Remove the dependency.
    uv_snapshot!(context.filters(), context.remove(&["child2"]).current_dir(&child1), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv remove` is experimental and may change without warning.
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

        [[distribution]]
        name = "child1"
        version = "0.1.0"
        source = "editable+child1"

        [[distribution]]
        name = "child2"
        version = "0.1.0"
        source = "editable+child2"
        "###
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().current_dir(&child1), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning.
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

        [[distribution]]
        name = "child1"
        version = "0.1.0"
        source = "editable+child1"

        [[distribution.dependencies]]
        name = "child2"

        [[distribution]]
        name = "child2"
        version = "0.1.0"
        source = "editable+child2"
        "###
        );
    });

    // Install from the lockfile.
    uv_snapshot!(context.filters(), context.sync().current_dir(&child1), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning.
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
    warning: `uv lock` is experimental and may change without warning.
    Resolved 6 packages in [TIME]
    "###);

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
     + urllib3==2.2.1
    "###);

    // Enable an extra (note the version specifier should be preserved).
    uv_snapshot!(context.filters(), context.add(&["requests[security]"]), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv add` is experimental and may change without warning.
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
    warning: `uv add` is experimental and may change without warning.
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

    // Change the source by specifying a version (note the extras should be preserved).
    uv_snapshot!(context.filters(), context.add(&["requests @ git+https://github.com/psf/requests"]).arg("--tag=v2.32.3"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv add` is experimental and may change without warning.
    warning: `uv.sources` is experimental and may change without warning.
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
            "requests[security,socks,use-chardet-on-py3] ; python_version > '3.7'",
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

        [[distribution]]
        name = "certifi"
        version = "2024.2.2"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/71/da/e94e26401b62acd6d91df2b52954aceb7f561743aa5ccc32152886c76c96/certifi-2024.2.2.tar.gz", hash = "sha256:0569859f95fc761b18b45ef421b1290a0f65f147e92a1e5eb3e635f9a5e4e66f", size = 164886 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/ba/06/a07f096c664aeb9f01624f858c3add0a4e913d6c96257acb4fce61e7de14/certifi-2024.2.2-py3-none-any.whl", hash = "sha256:dc383c07b76109f368f6106eee2b593b04a011ea4d55f652c6ca24a754d1cdd1", size = 163774 }]

        [[distribution]]
        name = "chardet"
        version = "5.2.0"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/f3/0d/f7b6ab21ec75897ed80c17d79b15951a719226b9fababf1e40ea74d69079/chardet-5.2.0.tar.gz", hash = "sha256:1b3b6ff479a8c414bc3fa2c0852995695c4a026dcd6d0633b2dd092ca39c1cf7", size = 2069618 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/38/6f/f5fbc992a329ee4e0f288c1fe0e2ad9485ed064cac731ed2fe47dcc38cbf/chardet-5.2.0-py3-none-any.whl", hash = "sha256:e1cf59446890a00105fe7b7912492ea04b6e6f06d4b742b2c788469e34c82970", size = 199385 }]

        [[distribution]]
        name = "charset-normalizer"
        version = "3.3.2"
        source = "registry+https://pypi.org/simple"
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
        	{ url = "https://files.pythonhosted.org/packages/28/76/e6222113b83e3622caa4bb41032d0b1bf785250607392e1b778aca0b8a7d/charset_normalizer-3.3.2-py3-none-any.whl", hash = "sha256:3e4d1f6587322d2788836a99c69062fbb091331ec940e02d12d179c1d53e25fc", size = 48543 }
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
        marker = "python_version > '3.7'"

        [[distribution.dependencies]]
        name = "requests"
        extra = "socks"
        marker = "python_version > '3.7'"

        [[distribution.dependencies]]
        name = "requests"
        extra = "use-chardet-on-py3"
        marker = "python_version > '3.7'"

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
        version = "2.32.3"
        source = "git+https://github.com/psf/requests?tag=v2.32.3#0e322af87745eff34caffe4df68456ebc20d9068"

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

        [[distribution.optional-dependencies.use-chardet-on-py3]]
        name = "chardet"

        [[distribution]]
        name = "urllib3"
        version = "2.2.1"
        source = "registry+https://pypi.org/simple"
        sdist = { url = "https://files.pythonhosted.org/packages/7a/50/7fd50a27caa0652cd4caf224aa87741ea41d3265ad13f010886167cfcc79/urllib3-2.2.1.tar.gz", hash = "sha256:d0570876c61ab9e520d776c38acbbb5b05a776d3f9ff98a5c8fd5162a444cf19", size = 291020 }
        wheels = [{ url = "https://files.pythonhosted.org/packages/a2/73/a68704750a7679d0b6d3ad7aa8d4da8e14e151ae82e6fee774e6e0d05ec8/urllib3-2.2.1-py3-none-any.whl", hash = "sha256:450b20ec296a467077128bff42b73080516e71b56ff59a60a02bef2232c4fa9d", size = 121067 }]
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
    Audited 8 packages in [TIME]
    "###);

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
    warning: `uv lock` is experimental and may change without warning.
    Resolved 4 packages in [TIME]
    "###);

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
    warning: `uv add` is experimental and may change without warning.
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

    // Install from the lockfile without cleaning the environment.
    uv_snapshot!(context.filters(), context.sync().arg("--no-clean"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning.
    Audited 2 packages in [TIME]
    "###);

    // Install from the lockfile, cleaning the environment.
    uv_snapshot!(context.filters(), context.sync(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv sync` is experimental and may change without warning.
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
    warning: `uv lock` is experimental and may change without warning.
    Resolved 4 packages in [TIME]
    "###);

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

    uv_snapshot!(context.filters(), context.remove(&["anyio"]), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv remove` is experimental and may change without warning.
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

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+."
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
    Audited 1 package in [TIME]
    "###);

    Ok(())
}

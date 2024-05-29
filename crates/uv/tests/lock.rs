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
    Resolved 4 packages in [TIME]
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

        [distribution.sdist]
        url = "https://files.pythonhosted.org/packages/c6/b3/fefbf7e78ab3b805dec67d698dc18dd505af7a18a8dd08868c9b4fa736b5/anyio-3.7.0.tar.gz"
        hash = "sha256:275d9973793619a5374e1c89a4f4ad3f4b0a5510a2b5b939444bee8f4c4d37ce"
        size = 142737

        [[distribution.wheel]]
        url = "https://files.pythonhosted.org/packages/68/fe/7ce1926952c8a403b35029e194555558514b365ad77d75125f521a2bec62/anyio-3.7.0-py3-none-any.whl"
        hash = "sha256:eddca883c4175f14df8aedce21054bfca3adb70ffe76a9f607aef9d7fa2ea7f0"
        size = 80873

        [[distribution.dependencies]]
        name = "idna"
        version = "3.6"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "idna"
        version = "3.6"
        source = "registry+https://pypi.org/simple"

        [distribution.sdist]
        url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz"
        hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca"
        size = 175426

        [[distribution.wheel]]
        url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl"
        hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f"
        size = 61567

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"

        [distribution.sdist]
        url = "file://[TEMP_DIR]/"

        [[distribution.dependencies]]
        name = "anyio"
        version = "3.7.0"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"

        [distribution.sdist]
        url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz"
        hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc"
        size = 20372

        [[distribution.wheel]]
        url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl"
        hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2"
        size = 10235
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

        [distribution.sdist]
        url = "file://[TEMP_DIR]/"

        [[distribution.dependencies]]
        name = "source-distribution"
        version = "0.0.1"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "source-distribution"
        version = "0.0.1"
        source = "registry+https://pypi.org/simple"

        [distribution.sdist]
        url = "https://files.pythonhosted.org/packages/10/1f/57aa4cce1b1abf6b433106676e15f9fa2c92ed2bd4cf77c3b50a9e9ac773/source_distribution-0.0.1.tar.gz"
        hash = "sha256:1f83ed7498336c7f2ab9b002cf22583d91115ebc624053dc4eb3a45694490106"
        size = 2157
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
        dependencies = ["anyio @ git+https://github.com/agronholm/anyio@3.7.0"]
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

        [[distribution]]
        name = "anyio"
        version = "3.7.0"
        source = "git+https://github.com/agronholm/anyio?rev=3.7.0#f7a880ffac4766efb39e6fb60fc28d944f5d2f65"

        [distribution.sdist]
        url = "https://github.com/agronholm/anyio?rev=3.7.0#f7a880ffac4766efb39e6fb60fc28d944f5d2f65"

        [[distribution.dependencies]]
        name = "idna"
        version = "3.6"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "idna"
        version = "3.6"
        source = "registry+https://pypi.org/simple"

        [distribution.sdist]
        url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz"
        hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca"
        size = 175426

        [[distribution.wheel]]
        url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl"
        hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f"
        size = 61567

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"

        [distribution.sdist]
        url = "file://[TEMP_DIR]/"

        [[distribution.dependencies]]
        name = "anyio"
        version = "3.7.0"
        source = "git+https://github.com/agronholm/anyio?rev=3.7.0#f7a880ffac4766efb39e6fb60fc28d944f5d2f65"

        [[distribution]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"

        [distribution.sdist]
        url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz"
        hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc"
        size = 20372

        [[distribution.wheel]]
        url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl"
        hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2"
        size = 10235
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
     + anyio==3.7.0 (from git+https://github.com/agronholm/anyio@f7a880ffac4766efb39e6fb60fc28d944f5d2f65)
     + idna==3.6
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + sniffio==1.3.1
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
    Resolved 4 packages in [TIME]
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

        [[distribution.wheel]]
        url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl"
        hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8"

        [[distribution.dependencies]]
        name = "idna"
        version = "3.6"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "idna"
        version = "3.6"
        source = "registry+https://pypi.org/simple"

        [distribution.sdist]
        url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz"
        hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca"
        size = 175426

        [[distribution.wheel]]
        url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl"
        hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f"
        size = 61567

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"

        [distribution.sdist]
        url = "file://[TEMP_DIR]/"

        [[distribution.dependencies]]
        name = "anyio"
        version = "4.3.0"
        source = "direct+https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl"

        [[distribution]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"

        [distribution.sdist]
        url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz"
        hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc"
        size = 20372

        [[distribution.wheel]]
        url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl"
        hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2"
        size = 10235
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
    Resolved 4 packages in [TIME]
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

        [distribution.sdist]
        url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz"
        hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6"

        [[distribution.dependencies]]
        name = "idna"
        version = "3.6"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "idna"
        version = "3.6"
        source = "registry+https://pypi.org/simple"

        [distribution.sdist]
        url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz"
        hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca"
        size = 175426

        [[distribution.wheel]]
        url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl"
        hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f"
        size = 61567

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"

        [distribution.sdist]
        url = "file://[TEMP_DIR]/"

        [[distribution.dependencies]]
        name = "anyio"
        version = "4.3.0"
        source = "direct+https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz"

        [[distribution]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"

        [distribution.sdist]
        url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz"
        hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc"
        size = 20372

        [[distribution.wheel]]
        url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl"
        hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2"
        size = 10235
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
        test = ["pytest"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `uv lock` is experimental and may change without warning.
    Resolved 8 packages in [TIME]
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

        [distribution.sdist]
        url = "https://files.pythonhosted.org/packages/c6/b3/fefbf7e78ab3b805dec67d698dc18dd505af7a18a8dd08868c9b4fa736b5/anyio-3.7.0.tar.gz"
        hash = "sha256:275d9973793619a5374e1c89a4f4ad3f4b0a5510a2b5b939444bee8f4c4d37ce"
        size = 142737

        [[distribution.wheel]]
        url = "https://files.pythonhosted.org/packages/68/fe/7ce1926952c8a403b35029e194555558514b365ad77d75125f521a2bec62/anyio-3.7.0-py3-none-any.whl"
        hash = "sha256:eddca883c4175f14df8aedce21054bfca3adb70ffe76a9f607aef9d7fa2ea7f0"
        size = 80873

        [[distribution.dependencies]]
        name = "idna"
        version = "3.6"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "idna"
        version = "3.6"
        source = "registry+https://pypi.org/simple"

        [distribution.sdist]
        url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz"
        hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca"
        size = 175426

        [[distribution.wheel]]
        url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl"
        hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f"
        size = 61567

        [[distribution]]
        name = "iniconfig"
        version = "2.0.0"
        source = "registry+https://pypi.org/simple"

        [distribution.sdist]
        url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz"
        hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3"
        size = 4646

        [[distribution.wheel]]
        url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl"
        hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374"
        size = 5892

        [[distribution]]
        name = "packaging"
        version = "24.0"
        source = "registry+https://pypi.org/simple"

        [distribution.sdist]
        url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz"
        hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9"
        size = 147882

        [[distribution.wheel]]
        url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl"
        hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5"
        size = 53488

        [[distribution]]
        name = "pluggy"
        version = "1.4.0"
        source = "registry+https://pypi.org/simple"

        [distribution.sdist]
        url = "https://files.pythonhosted.org/packages/54/c6/43f9d44d92aed815e781ca25ba8c174257e27253a94630d21be8725a2b59/pluggy-1.4.0.tar.gz"
        hash = "sha256:8c85c2876142a764e5b7548e7d9a0e0ddb46f5185161049a79b7e974454223be"
        size = 65812

        [[distribution.wheel]]
        url = "https://files.pythonhosted.org/packages/a5/5b/0cc789b59e8cc1bf288b38111d002d8c5917123194d45b29dcdac64723cc/pluggy-1.4.0-py3-none-any.whl"
        hash = "sha256:7db9f7b503d67d1c5b95f59773ebb58a8c1c288129a88665838012cfb07b8981"
        size = 20120

        [[distribution]]
        name = "project"
        version = "0.1.0"
        source = "editable+file://[TEMP_DIR]/"

        [distribution.sdist]
        url = "file://[TEMP_DIR]/"

        [[distribution.dependencies]]
        name = "anyio"
        version = "3.7.0"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "project"
        version = "0.1.0"
        extra = "test"
        source = "editable+file://[TEMP_DIR]/"

        [distribution.sdist]
        url = "file://[TEMP_DIR]/"

        [[distribution.dependencies]]
        name = "pytest"
        version = "8.1.1"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "pytest"
        version = "8.1.1"
        source = "registry+https://pypi.org/simple"

        [distribution.sdist]
        url = "https://files.pythonhosted.org/packages/30/b7/7d44bbc04c531dcc753056920e0988032e5871ac674b5a84cb979de6e7af/pytest-8.1.1.tar.gz"
        hash = "sha256:ac978141a75948948817d360297b7aae0fcb9d6ff6bc9ec6d514b85d5a65c044"
        size = 1409703

        [[distribution.wheel]]
        url = "https://files.pythonhosted.org/packages/4d/7e/c79cecfdb6aa85c6c2e3cf63afc56d0f165f24f5c66c03c695c4d9b84756/pytest-8.1.1-py3-none-any.whl"
        hash = "sha256:2a8386cfc11fa9d2c50ee7b2a57e7d898ef90470a7a34c4b949ff59662bb78b7"
        size = 337359

        [[distribution.dependencies]]
        name = "iniconfig"
        version = "2.0.0"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "packaging"
        version = "24.0"
        source = "registry+https://pypi.org/simple"

        [[distribution.dependencies]]
        name = "pluggy"
        version = "1.4.0"
        source = "registry+https://pypi.org/simple"

        [[distribution]]
        name = "sniffio"
        version = "1.3.1"
        source = "registry+https://pypi.org/simple"

        [distribution.sdist]
        url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz"
        hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc"
        size = 20372

        [[distribution.wheel]]
        url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl"
        hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2"
        size = 10235
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
    Downloaded 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + iniconfig==2.0.0
     + packaging==24.0
     + pluggy==1.4.0
     + pytest==8.1.1
    "###);

    Ok(())
}

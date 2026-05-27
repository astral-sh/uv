#[cfg(feature = "test-git")]
use std::process::Command;

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::*;
use async_zip::base::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
use futures::executor::block_on;
use insta::assert_snapshot;
use url::Url;

use uv_test::uv_snapshot;

fn package_section<'a>(lock: &'a str, name: &str) -> &'a str {
    let needle = format!("[[package]]\nname = \"{name}\"");
    let start = lock.find(&needle).expect("package section to exist");
    let rest = &lock[start..];
    let end = rest[1..]
        .find("\n[[package]]")
        .map(|offset| offset + 1)
        .unwrap_or(rest.len());
    &rest[..end]
}

/// Lock a project with a dependency that requires building from source
/// (due to dynamic metadata), and verify that build dependencies are captured
/// in the lock file.
#[test]
fn lock_build_dependencies() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    // Create a local dependency that uses setuptools with dynamic version,
    // forcing a build to extract metadata.
    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 4
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "dep"
        source = { directory = "dep" }
        build-dependencies = [
            { name = "setuptools", version = "69.2.0" },
        ]

        [package.metadata]
        build-requires = [{ name = "setuptools", specifier = ">=42" }]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "dep" },
        ]

        [package.metadata]
        requires-dist = [{ name = "dep", directory = "dep" }]

        [[package]]
        name = "setuptools"
        version = "69.2.0"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z" },
        ]
        "#
        );
    });

    // Re-run with `--locked` to ensure the lock file with build-dependencies is valid.
    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies").arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    ");

    Ok(())
}

/// Verify that build dependencies are resolved universally (for all platforms)
/// by using platform-specific markers on build requirements.
#[test]
fn lock_build_dependencies_universal() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    // Create a local dependency with platform-specific build dependencies.
    // With universal resolution, both anyio (linux) and iniconfig (darwin/windows)
    // should appear in the lock file regardless of the current platform.
    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = [
            "setuptools>=42",
            "wheel",
            "anyio ; sys_platform == 'linux'",
            "iniconfig ; sys_platform == 'darwin' or sys_platform == 'win32'",
        ]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");

    // With universal resolution, both anyio (linux) and iniconfig (darwin/windows)
    // should be captured as build dependencies, regardless of the current platform.
    // Also includes anyio's transitive deps (idna, sniffio).
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 4
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "dep"
        source = { directory = "dep" }
        build-dependencies = [
            { name = "anyio", version = "4.3.0", marker = "sys_platform == 'linux'" },
            { name = "iniconfig", version = "2.0.0", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [package.metadata]
        build-requires = [
            { name = "anyio", marker = "sys_platform == 'linux'" },
            { name = "iniconfig", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "setuptools", specifier = ">=42" },
            { name = "wheel" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "dep" },
        ]

        [package.metadata]
        requires-dist = [{ name = "dep", directory = "dep" }]

        [[package]]
        name = "setuptools"
        version = "69.2.0"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z" },
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
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z" },
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
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
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
        "#
        );
    });

    // Verify sync works with marker-filtered build dependencies.
    // On macOS, `iniconfig` (darwin/win32) is included and `anyio` (linux) is
    // filtered out; on Linux, the opposite would happen.
    #[cfg(not(windows))]
    {
        uv_snapshot!(context.filters(), context.sync().arg("--preview-features").arg("lock-build-dependencies"), @"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        Resolved 8 packages in [TIME]
        Prepared 1 package in [TIME]
        Installed 1 package in [TIME]
         + dep==0.1.0 (from file://[TEMP_DIR]/dep)
        ");
    }

    Ok(())
}

/// Verify that stored build dependencies are used as preferences during
/// subsequent resolves, producing the same versions.
#[test]
fn lock_build_dependencies_preference() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    // Create a local dependency with dynamic version requiring setuptools.
    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    // First lock to capture build dependencies.
    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock_initial = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock_initial, @r#"
        version = 1
        revision = 4
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "dep"
        source = { directory = "dep" }
        build-dependencies = [
            { name = "setuptools", version = "69.2.0" },
        ]

        [package.metadata]
        build-requires = [{ name = "setuptools", specifier = ">=42" }]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "dep" },
        ]

        [package.metadata]
        requires-dist = [{ name = "dep", directory = "dep" }]

        [[package]]
        name = "setuptools"
        version = "69.2.0"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z" },
        ]
        "#
        );
    });

    // Re-lock without `--locked` to verify build dependencies are preserved.
    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    ");

    let lock_second = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock_second, @r#"
        version = 1
        revision = 4
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "dep"
        source = { directory = "dep" }
        build-dependencies = [
            { name = "setuptools", version = "69.2.0" },
        ]

        [package.metadata]
        build-requires = [{ name = "setuptools", specifier = ">=42" }]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "dep" },
        ]

        [package.metadata]
        requires-dist = [{ name = "dep", directory = "dep" }]

        [[package]]
        name = "setuptools"
        version = "69.2.0"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z" },
        ]
        "#
        );
    });

    Ok(())
}

/// Lock a project with multiple local dependencies that each require building,
/// and verify each gets its own build-dependencies section.
#[test]
fn lock_build_dependencies_multiple_packages() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    // Create first local dependency.
    let dep_a_dir = context.temp_dir.child("dep-a");
    dep_a_dir.create_dir_all()?;
    dep_a_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep-a"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep_a.__version__"}
        "#,
    )?;
    dep_a_dir.child("dep_a").create_dir_all()?;
    dep_a_dir
        .child("dep_a/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    // Create second local dependency.
    let dep_b_dir = context.temp_dir.child("dep-b");
    dep_b_dir.create_dir_all()?;
    dep_b_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep-b"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep_b.__version__"}
        "#,
    )?;
    dep_b_dir.child("dep_b").create_dir_all()?;
    dep_b_dir
        .child("dep_b/__init__.py")
        .write_str("__version__ = '0.2.0'")?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep-a", "dep-b"]

        [tool.uv.sources]
        dep-a = { path = "dep-a" }
        dep-b = { path = "dep-b" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 4
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "dep-a"
        source = { directory = "dep-a" }
        build-dependencies = [
            { name = "setuptools", version = "69.2.0" },
        ]

        [package.metadata]
        build-requires = [{ name = "setuptools", specifier = ">=42" }]

        [[package]]
        name = "dep-b"
        source = { directory = "dep-b" }
        build-dependencies = [
            { name = "setuptools", version = "69.2.0" },
        ]

        [package.metadata]
        build-requires = [{ name = "setuptools", specifier = ">=42" }]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "dep-a" },
            { name = "dep-b" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "dep-a", directory = "dep-a" },
            { name = "dep-b", directory = "dep-b" },
        ]

        [[package]]
        name = "setuptools"
        version = "69.2.0"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z" },
        ]
        "#
        );
    });

    // Re-run with --locked to verify round-trip.
    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies").arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    ");

    Ok(())
}

/// Verify that `--upgrade` clears build dependency preferences and re-resolves
/// build dependencies from scratch.
#[test]
fn lock_build_dependencies_upgrade() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    // Initial lock.
    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock_initial = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock_initial, @r#"
        version = 1
        revision = 4
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "dep"
        source = { directory = "dep" }
        build-dependencies = [
            { name = "setuptools", version = "69.2.0" },
        ]

        [package.metadata]
        build-requires = [{ name = "setuptools", specifier = ">=42" }]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "dep" },
        ]

        [package.metadata]
        requires-dist = [{ name = "dep", directory = "dep" }]

        [[package]]
        name = "setuptools"
        version = "69.2.0"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z" },
        ]
        "#
        );
    });

    // Re-lock with `--upgrade` and a newer cutoff. The build dependency
    // preferences from the original lock should not hold `setuptools` back.
    context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--upgrade")
        .arg("--exclude-newer")
        .arg("2025-01-30T00:00:00Z")
        .assert()
        .success();

    let lock_upgraded = context.read("uv.lock");
    let dep = package_section(&lock_upgraded, "dep");
    assert!(
        dep.contains(r#"{ name = "setuptools", version = "75.8.0" }"#),
        "{dep}"
    );
    assert!(
        !dep.contains(r#"{ name = "setuptools", version = "69.2.0" }"#),
        "{dep}"
    );

    // Verify the lock file is valid.
    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies").arg("--locked").arg("--exclude-newer").arg("2025-01-30T00:00:00Z"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    Ok(())
}

/// Verify that `--exclude-newer` is respected for build dependency resolution.
#[test]
fn lock_build_dependencies_exclude_newer() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    // Lock with an exclude-newer date.
    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies").arg("--exclude-newer").arg("2024-03-25T00:00:00Z"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 4
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "dep"
        source = { directory = "dep" }
        build-dependencies = [
            { name = "setuptools", version = "69.2.0" },
        ]

        [package.metadata]
        build-requires = [{ name = "setuptools", specifier = ">=42" }]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "dep" },
        ]

        [package.metadata]
        requires-dist = [{ name = "dep", directory = "dep" }]

        [[package]]
        name = "setuptools"
        version = "69.2.0"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z" },
        ]
        "#
        );
    });

    // Verify the lock file is valid.
    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies").arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    ");

    Ok(())
}

/// Verify that `extra-build-dependencies` are included in the build dependency
/// resolution and captured in the lock file.
#[test]
fn lock_build_dependencies_extra() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep" }

        [tool.uv.extra-build-dependencies]
        dep = ["wheel"]
        "#,
    )?;

    // Lock with extra-build-dependencies (requires preview).
    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");

    let dep = package_section(&lock, "dep");
    assert!(
        dep.contains(r#"{ name = "iniconfig", version = "2.0.0" }"#),
        "{dep}"
    );
    assert!(dep.contains(r#"{ name = "setuptools", version = "69.2.0" }"#));
    assert!(lock.contains(r#"name = "iniconfig""#));

    Ok(())
}

/// Verify that `extra-build-dependencies` participate in lock freshness checks.
#[test]
fn lock_build_dependencies_extra_build_dependencies_invalidate() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep" }

        [tool.uv.extra-build-dependencies]
        dep = ["iniconfig"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies")
        .arg("--locked"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    The lockfile at `uv.lock` needs to be updated, but `--locked` was provided. To update the lockfile, run `uv lock`.
    ");

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Added iniconfig v2.0.0
    Removed wheel v0.43.0
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(dep.contains(r#"{ name = "iniconfig" }"#), "{dep}");

    Ok(())
}

/// Verify that configured extra build dependencies invalidate immutable
/// source-distribution locks when the configuration is removed.
#[test]
fn lock_build_dependencies_extra_build_dependencies_invalidate_find_links() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    let source_dist = links_dir.child("locked_extra_dep-0.1.0.zip");
    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new(
        "locked_extra_dep-0.1.0/pyproject.toml".into(),
        Compression::Stored,
    );
    block_on(zip.write_entry_whole(
        entry,
        br#"
        [project]
        name = "locked-extra-dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        build-backend = "setuptools.build_meta"
        "#,
    ))?;
    fs_err::write(source_dist.path(), block_on(zip.close())?)?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["locked-extra-dep==0.1.0"]

        [tool.uv.extra-build-dependencies]
        locked-extra-dep = ["setuptools>=42"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["locked-extra-dep==0.1.0"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies")
        .arg("--locked"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    The lockfile at `uv.lock` needs to be updated, but `--locked` was provided. To update the lockfile, run `uv lock`.
    ");

    Ok(())
}

/// Verify that universal build dependency locks use the project's Python
/// range, not just the interpreter used to generate the lock.
#[test]
fn lock_build_dependencies_use_project_python_range() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let builder_dir = context.temp_dir.child("builder");
    builder_dir.create_dir_all()?;
    builder_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "builder"
        version = "0.1.0"
        requires-python = ">=3.12"
        "#,
    )?;
    let builder_url = Url::from_directory_path(builder_dir.path()).unwrap();

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.8"

        [build-system]
        requires = ["setuptools>=42", "builder @ {builder_url}"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {{attr = "dep.__version__"}}
        "#
    ))?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.8"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to build `dep @ file://[TEMP_DIR]/dep`
      ├─▶ Failed to resolve requirements from `build-system.requires`
      ├─▶ No solution found when resolving: `setuptools>=42`, `builder @ file://[TEMP_DIR]/builder`
      ╰─▶ Because the requested Python version (>=3.8) does not satisfy Python>=3.12 and builder==0.1.0 depends on Python>=3.12, we can conclude that builder==0.1.0 cannot be used.
          And because only builder==0.1.0 is available and you require builder, we can conclude that your requirements are unsatisfiable.

          hint: The `requires-python` value (>=3.8) includes Python versions that are not supported by your dependencies (e.g., builder==0.1.0 only supports >=3.12). Consider using a more restrictive `requires-python` value (like >=3.12).
    ");

    Ok(())
}

/// Verify that universal build dependency locks respect the project's
/// supported marker environments.
#[test]
fn lock_build_dependencies_use_supported_environments() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let builder_dir = context.temp_dir.child("builder");
    builder_dir.create_dir_all()?;
    builder_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "builder"
        version = "0.1.0"
        requires-python = ">=3.13"
        "#,
    )?;
    let builder_url = Url::from_directory_path(builder_dir.path()).unwrap();

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42", "builder @ {builder_url}; sys_platform == 'win32'"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {{attr = "dep.__version__"}}
        "#
    ))?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv]
        environments = ["sys_platform == 'linux'"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    assert!(!lock.contains("[[package]]\nname = \"builder\""), "{lock}");

    Ok(())
}

/// Verify that PEP 508 extras in build requirements preserve their extra-only
/// dependency edges in the locked build environment.
#[test]
fn lock_build_dependencies_preserves_pep508_extras() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let leaf_dir = context.temp_dir.child("leaf");
    leaf_dir.create_dir_all()?;
    leaf_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "leaf"
        version = "0.1.0"
        requires-python = ">=3.12"
        "#,
    )?;
    let leaf_url = Url::from_directory_path(leaf_dir.path()).unwrap();

    let carrier_dir = context.temp_dir.child("carrier");
    carrier_dir.create_dir_all()?;
    let carrier_url = Url::from_directory_path(carrier_dir.path()).unwrap();
    carrier_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "carrier"
        version = "0.1.0"
        requires-python = ">=3.12"

        [project.optional-dependencies]
        extra = ["leaf @ {leaf_url}"]
        "#,
    ))?;

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42", "carrier[extra] @ {carrier_url}"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {{attr = "dep.__version__"}}
        "#,
    ))?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep", "carrier"]

        [tool.uv.sources]
        dep = { path = "dep" }
        carrier = { path = "carrier" }
        leaf = { path = "leaf" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(dep.contains(r#"name = "carrier""#), "{dep}");
    assert!(dep.contains(r#"extra = ["extra"]"#), "{dep}");

    let carrier = package_section(&lock, "carrier");
    assert!(carrier.contains("[package.optional-dependencies]"));
    assert!(carrier.contains(r#"{ name = "leaf" }"#));
    assert!(lock.contains(r#"name = "leaf""#));

    Ok(())
}

/// Verify that enabling build-dependency locking upgrades an otherwise
/// up-to-date revision-3 lock instead of returning it unchanged.
#[test]
fn lock_build_dependencies_relocks_revision_3() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    assert!(context.read("uv.lock").contains("revision = 3"));

    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Added setuptools v69.2.0
    Added wheel v0.43.0
    ");

    let lock = context.read("uv.lock");
    assert!(lock.contains("revision = 4"));
    assert!(package_section(&lock, "dep").contains("build-dependencies = ["));

    Ok(())
}

/// Verify that the shared default-backend cache records the default build
/// resolution for every source package that reuses it.
#[test]
fn lock_build_dependencies_default_backend_cache_records_each_package() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let dep_a_dir = context.temp_dir.child("dep-a");
    dep_a_dir.create_dir_all()?;
    dep_a_dir
        .child("setup.py")
        .write_str("from setuptools import setup\nsetup(name='dep-a', version='0.1.0')\n")?;

    let dep_b_dir = context.temp_dir.child("dep-b");
    dep_b_dir.create_dir_all()?;
    dep_b_dir
        .child("setup.py")
        .write_str("from setuptools import setup\nsetup(name='dep-b', version='0.1.0')\n")?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep-a", "dep-b"]

        [tool.uv.sources]
        dep-a = { path = "dep-a" }
        dep-b = { path = "dep-b" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    assert!(package_section(&lock, "dep-a").contains("build-dependencies = ["));
    assert!(package_section(&lock, "dep-b").contains("build-dependencies = ["));

    Ok(())
}

/// Verify that a static source package without an explicit build system still
/// records PEP 517's implicit setuptools build requirement.
#[test]
fn lock_build_dependencies_static_directory_implicit_default_backend() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir.child("dep/__init__.py").touch()?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(dep.contains("build-dependencies = ["), "{dep}");
    assert!(dep.contains(r#"{ name = "setuptools", version = "69.2.0" }"#));

    Ok(())
}

/// Verify that static Git dependencies retain their declared build
/// requirements in the locked build environment.
#[test]
#[cfg(feature = "test-git")]
fn lock_build_dependencies_static_git() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;
    Command::new("git")
        .arg("init")
        .arg("--quiet")
        .arg(dep_dir.path())
        .assert()
        .success();
    Command::new("git")
        .args(["-C", dep_dir.path().to_str().expect("UTF-8 temp path")])
        .args(["config", "user.name", "uv-test"])
        .assert()
        .success();
    Command::new("git")
        .args(["-C", dep_dir.path().to_str().expect("UTF-8 temp path")])
        .args(["config", "user.email", "uv-test@example.com"])
        .assert()
        .success();
    Command::new("git")
        .args(["-C", dep_dir.path().to_str().expect("UTF-8 temp path")])
        .args(["add", "pyproject.toml"])
        .assert()
        .success();
    Command::new("git")
        .args(["-C", dep_dir.path().to_str().expect("UTF-8 temp path")])
        .args(["commit", "--quiet", "-m", "initial"])
        .assert()
        .success();

    let dep_url = Url::from_directory_path(dep_dir.path()).expect("valid file URL");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&format!(
            r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = {{ git = "{dep_url}" }}
        "#
        ))?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(dep.contains("build-dependencies = ["), "{dep}");
    assert!(dep.contains(r#"{ name = "setuptools", version = "69.2.0" }"#));

    Ok(())
}

/// Verify that build dependencies are captured correctly when the resolver forks
/// due to platform-specific dependencies.
#[test]
fn lock_build_dependencies_fork() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    // Create a local dependency with dynamic version, forcing a build.
    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "dep",
            "iniconfig>=1 ; sys_platform == 'linux'",
            "iniconfig>=2 ; sys_platform == 'win32'",
        ]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 4
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform == 'linux'",
            "sys_platform == 'win32'",
            "sys_platform != 'linux' and sys_platform != 'win32'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "dep"
        source = { directory = "dep" }
        build-dependencies = [
            { name = "setuptools", version = "69.2.0" },
        ]

        [package.metadata]
        build-requires = [{ name = "setuptools", specifier = ">=42" }]

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
            { name = "dep" },
            { name = "iniconfig", marker = "sys_platform == 'linux' or sys_platform == 'win32'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "dep", directory = "dep" },
            { name = "iniconfig", marker = "sys_platform == 'linux'", specifier = ">=1" },
            { name = "iniconfig", marker = "sys_platform == 'win32'", specifier = ">=2" },
        ]

        [[package]]
        name = "setuptools"
        version = "69.2.0"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z" },
        ]
        "#
        );
    });

    // Verify the lock file is valid.
    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies").arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    ");

    Ok(())
}

/// Verify that a package appearing in both the runtime dependency tree and the
/// build dependency tree is not duplicated, and its existing `dependencies`
/// from the main resolution are preserved.
#[test]
fn lock_build_dependencies_shared_package() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    // Create a local dependency with dynamic version that requires `iniconfig`
    // as a build dependency (via setuptools build-system.requires).
    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42", "iniconfig", "wheel"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    // The root project depends on both `dep` (which needs iniconfig to build)
    // and `iniconfig` directly as a runtime dependency.
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep", "iniconfig"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    let lock = context.read("uv.lock");

    // `iniconfig` should appear exactly once as a [[package]], used by both
    // the build-dependencies of `dep` and the runtime dependencies of `project`.
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 4
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "dep"
        source = { directory = "dep" }
        build-dependencies = [
            { name = "iniconfig", version = "2.0.0" },
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [package.metadata]
        build-requires = [
            { name = "iniconfig" },
            { name = "setuptools", specifier = ">=42" },
            { name = "wheel" },
        ]

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
            { name = "dep" },
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "dep", directory = "dep" },
            { name = "iniconfig" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z" },
        ]

        [[package]]
        name = "setuptools"
        version = "69.2.0"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z" },
        ]
        "#
        );
    });

    // Verify the lock file is valid by re-locking with --locked.
    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies").arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    ");

    // Verify sync works (the shared package should be installed once).
    uv_snapshot!(context.filters(), context.sync().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + dep==0.1.0 (from file://[TEMP_DIR]/dep)
     + iniconfig==2.0.0
    ");

    Ok(())
}

/// Verify that changing `build-system.requires` in a dependency's pyproject.toml
/// invalidates the lock file and triggers re-resolution.
#[test]
fn lock_build_dependencies_stale_build_requires() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    // Run the initial lock.
    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    // `--locked` should pass.
    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    ");

    // Change build-system.requires to add `wheel`.
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42", "wheel"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;

    // --locked should now fail because build-system.requires changed.
    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--locked"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    The lockfile at `uv.lock` needs to be updated, but `--locked` was provided. To update the lockfile, run `uv lock`.
    ");

    // Re-lock without `--locked` to pick up the new `build-system.requires`.
    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    // `--locked` should pass now.
    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    ");

    // The `build-requires` should be updated in the lock file.
    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 4
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "dep"
        source = { directory = "dep" }
        build-dependencies = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [package.metadata]
        build-requires = [
            { name = "setuptools", specifier = ">=42" },
            { name = "wheel" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "dep" },
        ]

        [package.metadata]
        requires-dist = [{ name = "dep", directory = "dep" }]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z" },
        ]

        [[package]]
        name = "setuptools"
        version = "69.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z" },
        ]
        "#
        );
    });

    Ok(())
}

/// Verify that transitive build dependencies behind platform markers are
/// correctly excluded at sync time. When `anyio` (linux-only build dep)
/// is skipped on macOS/Windows, its transitive deps `idna` and `sniffio`
/// must also be excluded from the build environment.
#[test]
fn lock_build_dependencies_transitive_marker_filtering() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    // Create a local dependency with platform-specific build dependencies.
    // `anyio` is linux-only, `iniconfig` is darwin/windows-only.
    // `anyio` depends on `idna` and `sniffio`.
    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = [
            "setuptools>=42",
            "wheel",
            "anyio ; sys_platform == 'linux'",
            "iniconfig ; sys_platform == 'darwin' or sys_platform == 'win32'",
        ]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");

    // Verify the lock file structure:
    // - build-dependencies only has direct deps (setuptools, wheel, anyio, iniconfig)
    // - anyio has dependencies = [idna, sniffio]
    // - idna and sniffio are NOT in build-dependencies
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 4
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "dep"
        source = { directory = "dep" }
        build-dependencies = [
            { name = "anyio", version = "4.3.0", marker = "sys_platform == 'linux'" },
            { name = "iniconfig", version = "2.0.0", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [package.metadata]
        build-requires = [
            { name = "anyio", marker = "sys_platform == 'linux'" },
            { name = "iniconfig", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "setuptools", specifier = ">=42" },
            { name = "wheel" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "dep" },
        ]

        [package.metadata]
        requires-dist = [{ name = "dep", directory = "dep" }]

        [[package]]
        name = "setuptools"
        version = "69.2.0"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z" },
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
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z" },
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
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
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
        "#
        );
    });

    // Sync should succeed with transitive marker filtering applied.
    // The lock snapshot above is the primary assertion for marker filtering:
    // transitive packages (`idna`, `sniffio`) are present only under `anyio`
    // dependencies, not as direct build dependencies.
    uv_snapshot!(context.filters(), context.sync().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + dep==0.1.0 (from file://[TEMP_DIR]/dep)
    ");

    Ok(())
}

/// Verify that build dependencies work for a directory dependency with a dynamic
/// version when syncing. During `uv lock`, the build resolution is stored with
/// the resolved version from the lock file. During `uv sync`, directory dists
/// return a URL (not a version) from `version_or_url()`, so `package_version` is
/// `None` at build time. The lookup must handle this version-key mismatch.
#[test]
fn lock_build_dependencies_dynamic_version_directory() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42", "wheel"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(
            lock, @r#"
        version = 1
        revision = 4
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "dep"
        source = { directory = "dep" }
        build-dependencies = [
            { name = "setuptools", version = "69.2.0" },
            { name = "wheel", version = "0.43.0" },
        ]

        [package.metadata]
        build-requires = [
            { name = "setuptools", specifier = ">=42" },
            { name = "wheel" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "dep" },
        ]

        [package.metadata]
        requires-dist = [{ name = "dep", directory = "dep" }]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z" },
        ]

        [[package]]
        name = "setuptools"
        version = "69.2.0"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z" },
        ]
        "#
        );
    });

    // Sync should succeed and use the locked build resolutions even though
    // `package_version` is `None` for the directory dep at build time.
    uv_snapshot!(context.filters(), context.sync().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + dep==0.1.0 (from file://[TEMP_DIR]/dep)
    ");

    Ok(())
}

/// Verify that `--no-build` disables build dependency locking.
#[test]
fn lock_build_dependencies_no_build_disables_locking() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["iniconfig"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-build"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(lock, @r#"
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
        requires-dist = [{ name = "iniconfig" }]
        "#);
    });

    Ok(())
}

/// Verify that a lock created with `--no-build` is not reused when build
/// dependency locking is later requested without `--no-build`.
#[test]
fn lock_build_dependencies_no_build_relocks_without_no_build() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-build"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    assert!(!lock.contains("revision = 4"));
    assert!(!lock.contains("build-dependencies = ["));

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--locked"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    The lockfile at `uv.lock` needs to be updated, but `--locked` was provided. To update the lockfile, run `uv lock`.
    ");

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Added setuptools v69.2.0
    ");

    let lock = context.read("uv.lock");
    assert!(lock.contains("revision = 4"));
    assert!(lock.contains("build-dependencies = ["));

    Ok(())
}

/// Verify that path source distributions with static metadata still have
/// their build requirements locked.
#[test]
fn lock_build_dependencies_static_sdist() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let source_dist = context.temp_dir.child("dep-0.1.0.zip");
    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new("dep-0.1.0/pyproject.toml".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    ))?;
    let entry = ZipEntryBuilder::new("dep-0.1.0/dep/__init__.py".into(), Compression::Stored);
    block_on(zip.write_entry_whole(entry, b""))?;
    fs_err::write(source_dist.path(), block_on(zip.close())?)?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep-0.1.0.zip" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(dep.contains("build-dependencies = ["), "{dep}");
    assert!(dep.contains(r#"{ name = "setuptools", version = "69.2.0" }"#));

    Ok(())
}

/// Verify that changing build requirements in a mutable local source archive
/// invalidates the locked build environment.
#[test]
fn lock_build_dependencies_static_sdist_build_requires_invalidate() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let source_dist = context.temp_dir.child("dep-0.1.0.zip");
    let write_source_dist = |requires: &str| -> Result<()> {
        let mut zip = ZipFileWriter::new(Vec::new());
        let entry = ZipEntryBuilder::new("dep-0.1.0/pyproject.toml".into(), Compression::Stored);
        let pyproject_toml = format!(
            r#"
                [project]
                name = "dep"
                version = "0.1.0"
                requires-python = ">=3.12"

                [build-system]
                requires = [{requires}]
                build-backend = "setuptools.build_meta"
                "#
        );
        block_on(zip.write_entry_whole(entry, pyproject_toml.as_bytes()))?;
        let entry = ZipEntryBuilder::new("dep-0.1.0/dep/__init__.py".into(), Compression::Stored);
        block_on(zip.write_entry_whole(entry, b""))?;
        fs_err::write(source_dist.path(), block_on(zip.close())?)?;
        Ok(())
    };
    write_source_dist(r#""setuptools>=42""#)?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep-0.1.0.zip" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(dep.contains(r#"build-requires = [{ name = "setuptools", specifier = ">=42" }]"#));

    write_source_dist(r#""setuptools>=42", "wheel""#)?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--locked"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    The lockfile at `uv.lock` needs to be updated, but `--locked` was provided. To update the lockfile, run `uv lock`.
    ");

    Ok(())
}

/// Verify that `--no-build-package` skips build dependency locking only for
/// the selected package.
#[test]
fn lock_build_dependencies_no_build_package_skips_selected() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    for dep_name in ["dep", "dep2"] {
        let dep_dir = context.temp_dir.child(dep_name);
        dep_dir.create_dir_all()?;
        dep_dir.child("pyproject.toml").write_str(&format!(
            r#"
            [project]
            name = "{dep_name}"
            version = "0.1.0"
            requires-python = ">=3.12"

            [build-system]
            requires = ["setuptools>=42"]
            build-backend = "setuptools.build_meta"
            "#
        ))?;
    }

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep", "dep2"]

        [tool.uv.sources]
        dep = { path = "dep" }
        dep2 = { path = "dep2" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-build-package")
        .arg("dep"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(lock, @r#"
        version = 1
        revision = 4
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "dep"
        version = "0.1.0"
        source = { directory = "dep" }

        [package.metadata]
        build-requires = [{ name = "setuptools", specifier = ">=42" }]

        [[package]]
        name = "dep2"
        version = "0.1.0"
        source = { directory = "dep2" }
        build-dependencies = [
            { name = "setuptools", version = "69.2.0" },
        ]

        [package.metadata]
        build-requires = [{ name = "setuptools", specifier = ">=42" }]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "dep" },
            { name = "dep2" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "dep", directory = "dep" },
            { name = "dep2", directory = "dep2" },
        ]

        [[package]]
        name = "setuptools"
        version = "69.2.0"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "wheel", version = "0.43.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4d/5b/dc575711b6b8f2f866131a40d053e30e962e633b332acf7cd2c24843d83d/setuptools-69.2.0.tar.gz", hash = "sha256:0ff4183f8f42cd8fa3acea16c45205521a4ef28f73c6391d8a25e92893134f2e", size = 2222950, upload-time = "2024-03-13T11:20:59.219Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/92/e1/1c8bb3420105e70bdf357d57dd5567202b4ef8d27f810e98bb962d950834/setuptools-69.2.0-py3-none-any.whl", hash = "sha256:c21c49fb1042386df081cb5d86759792ab89efca84cf114889191cd09aacc80c", size = 821485, upload-time = "2024-03-13T11:20:54.103Z" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z" },
        ]
        "#);
    });

    Ok(())
}

/// Verify that sync only reconstructs locked build resolutions for selected
/// packages, not unselected optional dependencies.
#[test]
fn sync_filters_locked_build_resolutions_to_selected_packages() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let helper_dir = context.temp_dir.child("helper");
    helper_dir.create_dir_all()?;
    helper_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "helper"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "helper.__version__"}
        "#,
    )?;
    helper_dir.child("helper").create_dir_all()?;
    helper_dir
        .child("helper/__init__.py")
        .write_str("__version__ = '0.1.0'")?;
    let helper_url = Url::from_directory_path(helper_dir.path()).unwrap();

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["helper @ {helper_url}"]
        build-backend = "setuptools.build_meta"
        "#
    ))?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [dependency-groups]
        dev = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep" }
        helper = { path = "helper" }

        [tool.uv]
        package = false
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--frozen")
        .arg("--no-default-groups")
        .arg("--no-build"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Checked in [TIME]
    ");

    Ok(())
}

/// Verify that lock-time metadata builds use the build dependency branch for
/// the active marker environment instead of flattening every universal branch
/// into one concrete build environment.
#[test]
fn lock_build_dependencies_marker_selected_build_environment() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let other_builder_dir = context.temp_dir.child("builder-other");
    other_builder_dir.create_dir_all()?;
    other_builder_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "builder"
        version = "0.2.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;
    other_builder_dir.child("builder").create_dir_all()?;
    other_builder_dir
        .child("builder/__init__.py")
        .write_str("DEP_VERSION = '0.2.0'")?;
    let other_builder_url = Url::from_directory_path(other_builder_dir.path()).unwrap();

    let linux_builder_dir = context.temp_dir.child("builder-linux");
    linux_builder_dir.create_dir_all()?;
    linux_builder_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "builder"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;
    linux_builder_dir.child("builder").create_dir_all()?;
    linux_builder_dir
        .child("builder/__init__.py")
        .write_str("DEP_VERSION = '0.1.0'")?;
    let linux_builder_url = Url::from_directory_path(linux_builder_dir.path()).unwrap();

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = [
            "setuptools>=42",
            "builder @ {other_builder_url} ; sys_platform != 'linux'",
            "builder @ {linux_builder_url} ; sys_platform == 'linux'",
        ]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {{attr = "dep.__version__"}}
        "#
    ))?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("from builder import DEP_VERSION\n__version__ = DEP_VERSION")?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    let expected_version = if cfg!(target_os = "linux") {
        "version = \"0.1.0\""
    } else {
        "version = \"0.2.0\""
    };
    assert!(dep.contains(expected_version), "{dep}");

    Ok(())
}

/// Verify that source packages reached only through build dependencies are
/// reconstructed for frozen syncs and validated by locked relocks.
#[test]
fn lock_build_dependencies_build_only_source_package() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let builder_dir = context.temp_dir.child("builder");
    builder_dir.create_dir_all()?;
    let builder_pyproject = builder_dir.child("pyproject.toml");
    builder_pyproject.write_str(
        r#"
        [project]
        name = "builder"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42", "wheel"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "builder.__version__"}
        "#,
    )?;
    builder_dir.child("builder").create_dir_all()?;
    builder_dir
        .child("builder/__init__.py")
        .write_str("__version__ = '0.1.0'")?;
    let builder_url = Url::from_directory_path(builder_dir.path()).unwrap();

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42", "builder @ {builder_url}"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {{attr = "dep.__version__"}}
        "#
    ))?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let builder = package_section(&lock, "builder");
    assert!(builder.contains("build-dependencies = ["), "{builder}");

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    ");

    let missing_url = Url::from_directory_path(context.temp_dir.child("missing").path()).unwrap();
    builder_pyproject.write_str(&format!(
        r#"
        [project]
        name = "builder"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["missing @ {missing_url}"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {{attr = "builder.__version__"}}
        "#
    ))?;

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--frozen"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + dep==0.1.0 (from file://[TEMP_DIR]/dep)
    ");

    builder_pyproject.write_str(
        r#"
        [project]
        name = "builder"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42", "wheel"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "builder.__version__"}
        "#,
    )?;
    builder_dir
        .child("builder/__init__.py")
        .write_str("__version__ = '0.2.0'")?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    ");

    Ok(())
}

/// Verify that relocking without the preview feature preserves existing
/// locked build dependencies without churn.
#[test]
fn lock_build_dependencies_on_then_off_no_churn() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock_with_feature = context.read("uv.lock");
    assert!(lock_with_feature.contains("build-dependencies = ["));

    uv_snapshot!(context.filters(), context.lock(), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 4 packages in [TIME]
    ");

    let lock_without_feature = context.read("uv.lock");
    assert_eq!(lock_without_feature, lock_with_feature);

    Ok(())
}

/// Verify that when relocking without the preview feature requires a rewrite,
/// build dependencies are not emitted by the non-preview path.
#[test]
fn lock_build_dependencies_on_then_off_forced_rewrite() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42"]
        build-backend = "setuptools.build_meta"

        [tool.setuptools.dynamic]
        version = {attr = "dep.__version__"}
        "#,
    )?;
    dep_dir.child("dep").create_dir_all()?;
    dep_dir
        .child("dep/__init__.py")
        .write_str("__version__ = '0.1.0'")?;

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    // Force a rewrite by changing project dependencies.
    pyproject_toml.write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep", "iniconfig"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    Added iniconfig v2.0.0
    Removed setuptools v69.2.0
    Removed wheel v0.43.0
    ");

    let lock = context.read("uv.lock");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "dep"
        source = { directory = "dep" }

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
            { name = "dep" },
            { name = "iniconfig" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "dep", directory = "dep" },
            { name = "iniconfig" },
        ]
        "#);
    });

    Ok(())
}

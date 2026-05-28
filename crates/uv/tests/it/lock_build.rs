#[cfg(feature = "test-git")]
use std::process::Command;

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::ChildPath;
use assert_fs::prelude::*;
use async_zip::base::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
use futures::executor::block_on;
use insta::assert_snapshot;
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

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

fn write_wheel(path: &ChildPath, name: &str, version: &str) -> Result<()> {
    write_wheel_with_requires(path, name, version, &[])
}

fn write_wheel_with_requires(
    path: &ChildPath,
    name: &str,
    version: &str,
    requires_dist: &[&str],
) -> Result<()> {
    let mut zip = ZipFileWriter::new(Vec::new());
    let dist_info = format!("{}-{version}.dist-info", name.replace('-', "_"));

    let entry = ZipEntryBuilder::new(
        format!("{}.py", name.replace('-', "_")).into(),
        Compression::Stored,
    );
    block_on(zip.write_entry_whole(entry, b""))?;
    let entry = ZipEntryBuilder::new(format!("{dist_info}/METADATA").into(), Compression::Stored);
    let mut metadata = format!("Metadata-Version: 2.3\nName: {name}\nVersion: {version}\n");
    for requirement in requires_dist {
        metadata.push_str(&format!("Requires-Dist: {requirement}\n"));
    }
    block_on(zip.write_entry_whole(entry, metadata.as_bytes()))?;
    let entry = ZipEntryBuilder::new(format!("{dist_info}/WHEEL").into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        b"Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
    ))?;
    let entry = ZipEntryBuilder::new(format!("{dist_info}/RECORD").into(), Compression::Stored);
    block_on(zip.write_entry_whole(entry, b""))?;
    fs_err::write(path.path(), block_on(zip.close())?)?;

    Ok(())
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
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        build-dependencies = [
            { name = "setuptools", version = "69.2.0", marker = "sys_platform == 'linux'" },
            { name = "setuptools-scm", version = "8.0.4", marker = "sys_platform == 'linux'" },
            { name = "wheel", version = "0.43.0", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642, upload-time = "2024-02-19T08:36:28.641Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584, upload-time = "2024-02-19T08:36:26.842Z" },
        ]

        [[package]]
        name = "calver"
        version = "2022.6.26"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "setuptools", version = "69.2.0", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "wheel", version = "0.43.0", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b5/00/96cbed7c019c49ee04b8a08357a981983db7698ae6de402e57097cefc9ad/calver-2022.6.26.tar.gz", hash = "sha256:e05493a3b17517ef1748fbe610da11f10485faa7c416b9d33fd4a52d74894f8b", size = 6670, upload-time = "2022-06-26T23:25:10.382Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f7/39/e421c06f42ca00fa9cf8929c2466e58a837e8e97b8ab3ff4f4ff9a15e33e/calver-2022.6.26-py3-none-any.whl", hash = "sha256:a1d7fcdd67797afc52ee36ffb8c8adf6643173864306547bfd1380cbce6310a0", size = 7049, upload-time = "2022-06-26T23:25:07.692Z" },
        ]

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
        name = "flit-core"
        version = "3.9.0"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = []
        sdist = { url = "https://files.pythonhosted.org/packages/c4/e6/c1ac50fe3eebb38a155155711e6e864e254ce4b6e17fe2429b4c4d5b9e80/flit_core-3.9.0.tar.gz", hash = "sha256:72ad266176c4a3fcfab5f2930d76896059851240570ce9a98733b658cb786eba", size = 41917, upload-time = "2023-05-14T14:48:51.809Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/38/45/618e84e49a6c51e5dd15565ec2fcd82ab273434f236b8f108f065ded517a/flit_core-3.9.0-py3-none-any.whl", hash = "sha256:7aada352fb0c7f5538c4fafeddf314d3a6a92ee8e2b1de70482329e42de70301", size = 63141, upload-time = "2023-05-14T14:48:49.24Z" },
        ]

        [[package]]
        name = "hatch-vcs"
        version = "0.4.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "hatchling", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "setuptools-scm", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
        ]
        build-dependencies = [
            { name = "hatchling", version = "1.22.4", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/f5/c9/54bb4fa27b4e4a014ef3bb17710cdf692b3aa2cbc7953da885f1bf7e06ea/hatch_vcs-0.4.0.tar.gz", hash = "sha256:093810748fe01db0d451fabcf2c1ac2688caefd232d4ede967090b1c1b07d9f7", size = 10917, upload-time = "2023-11-06T06:24:57.228Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/82/0f/6cbd9976160bc334add63bc2e7a58b1433a31b34b7cda6c5de6dd983d9a7/hatch_vcs-0.4.0-py3-none-any.whl", hash = "sha256:b8a2b6bee54cf6f9fc93762db73890017ae59c9081d1038a41f16235ceaf8b2c", size = 8412, upload-time = "2023-11-06T06:24:55.389Z" },
        ]

        [[package]]
        name = "hatchling"
        version = "1.22.4"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "packaging", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "pathspec", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "pluggy", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "trove-classifiers", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
        ]
        build-dependencies = [
            { name = "packaging", version = "24.0", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "pathspec", version = "0.12.1", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "pluggy", version = "1.4.0", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "trove-classifiers", version = "2024.3.3", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4f/2a/c34d71531d1e1c9a5029bb73eb3816285befd0fffd7c63ffa0544253dca8/hatchling-1.22.4.tar.gz", hash = "sha256:8a2dcec96d7fb848382ef5848e5ac43fdae641f35a08a3fab5116bd495f3416e", size = 62758, upload-time = "2024-03-24T02:00:59.122Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/63/2d56d6356f9f8b906aa68335cbf5b1b54c69873a2e271eda2ddba319c1ae/hatchling-1.22.4-py3-none-any.whl", hash = "sha256:f56da5bfc396af7b29daa3164851dd04991c994083f56cb054b5003675caecdc", size = 82032, upload-time = "2024-03-24T02:00:57.534Z" },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "flit-core", version = "3.9.0", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426, upload-time = "2023-11-25T15:40:54.902Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567, upload-time = "2023-11-25T15:40:52.604Z" },
        ]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "hatch-vcs", version = "0.4.0", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "hatchling", version = "1.22.4", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "flit-core", version = "3.9.0", marker = "sys_platform == 'darwin' or sys_platform == 'linux' or sys_platform == 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "flit-core", version = "3.9.0", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "pluggy"
        version = "1.4.0"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "setuptools", version = "69.2.0", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "setuptools-scm", version = "8.0.4", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "setuptools-scm", version = "8.0.4", extra = ["toml"], marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "wheel", version = "0.43.0", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/54/c6/43f9d44d92aed815e781ca25ba8c174257e27253a94630d21be8725a2b59/pluggy-1.4.0.tar.gz", hash = "sha256:8c85c2876142a764e5b7548e7d9a0e0ddb46f5185161049a79b7e974454223be", size = 65812, upload-time = "2024-01-24T13:45:15.875Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a5/5b/0cc789b59e8cc1bf288b38111d002d8c5917123194d45b29dcdac64723cc/pluggy-1.4.0-py3-none-any.whl", hash = "sha256:7db9f7b503d67d1c5b95f59773ebb58a8c1c288129a88665838012cfb07b8981", size = 20120, upload-time = "2024-01-24T13:45:14.227Z" },
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
        name = "setuptools-scm"
        version = "8.0.4"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "packaging", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "packaging", marker = "sys_platform == 'linux'" },
            { name = "setuptools", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "setuptools", marker = "sys_platform == 'linux'" },
            { name = "typing-extensions", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "typing-extensions", marker = "sys_platform == 'linux'" },
        ]
        build-dependencies = [
            { name = "setuptools", version = "69.2.0", marker = "sys_platform == 'darwin' or sys_platform == 'linux' or sys_platform == 'win32'" },
            { name = "wheel", version = "0.43.0", marker = "sys_platform == 'darwin' or sys_platform == 'linux' or sys_platform == 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/eb/b1/0248705f10f6de5eefe7ff93e399f7192257b23df4d431d2f5680bb2778f/setuptools-scm-8.0.4.tar.gz", hash = "sha256:b5f43ff6800669595193fd09891564ee9d1d7dcb196cab4b2506d53a2e1c95c7", size = 74280, upload-time = "2023-10-02T15:14:32.996Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/0e/a3/b9a8b0adfe672bf0df5901707aa929d30a97ee390ba651910186776746d2/setuptools_scm-8.0.4-py3-none-any.whl", hash = "sha256:b47844cd2a84b83b3187a5782c71128c28b4c94cad8bfb871da2784a5cb54c4f", size = 42137, upload-time = "2023-10-02T15:14:31.281Z" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "setuptools", version = "69.2.0", marker = "sys_platform == 'linux'" },
            { name = "setuptools-scm", version = "8.0.4", marker = "sys_platform == 'linux'" },
            { name = "wheel", version = "0.43.0", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372, upload-time = "2024-02-25T23:20:04.057Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235, upload-time = "2024-02-25T23:20:01.196Z" },
        ]

        [[package]]
        name = "trove-classifiers"
        version = "2024.3.3"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "calver", version = "2022.6.26", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "setuptools", version = "69.2.0", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "wheel", version = "0.43.0", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/13/11/e13906315b498cb8f5ce5a7ff39fc35941e8291e914158157937fd1c095d/trove-classifiers-2024.3.3.tar.gz", hash = "sha256:df7edff9c67ff86b733628998330b180e81d125b1e096536d83ac0fd79673fdc", size = 15982, upload-time = "2024-03-03T20:17:38.634Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/bb/81/a16cb58f719e68d0cce72fb9afd6f0f50c0e474d7b8dc267c8309c3e2793/trove_classifiers-2024.3.3-py3-none-any.whl", hash = "sha256:3a84096861b385ec422c79995d1f6435dde47a9b63adaa3c886e53232ba7e6e0", size = 13377, upload-time = "2024-03-03T20:17:34.101Z" },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "flit-core", version = "3.9.0", marker = "sys_platform == 'darwin' or sys_platform == 'linux' or sys_platform == 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558, upload-time = "2024-02-25T22:12:49.693Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926, upload-time = "2024-02-25T22:12:47.72Z" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z" },
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
        Resolved 18 packages in [TIME]
        Prepared 1 package in [TIME]
        Installed 1 package in [TIME]
         + dep==0.1.0 (from file://[TEMP_DIR]/dep)
        ");
    }

    Ok(())
}

/// Verify that a shared source build dependency is locked for every marker
/// region from which it can be reached.
#[test]
fn lock_build_dependencies_shared_source_widens_marker_reachability() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("seed_linux-0.1.0-py3-none-any.whl"),
        "seed-linux",
        "0.1.0",
    )?;
    write_wheel(
        &links_dir.child("seed_win-0.1.0-py3-none-any.whl"),
        "seed-win",
        "0.1.0",
    )?;

    let builder_dir = context.temp_dir.child("builder");
    builder_dir.create_dir_all()?;
    builder_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "builder"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = [
            "seed-linux==0.1.0 ; sys_platform == 'linux'",
            "seed-win==0.1.0 ; sys_platform == 'win32'",
        ]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    builder_dir.child("build_backend.py").write_str(
        r#"
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "builder-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("builder/__init__.py", "")
        wheel.writestr(
            "builder-0.1.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: builder\nVersion: 0.1.0\n",
        )
        wheel.writestr(
            "builder-0.1.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("builder-0.1.0.dist-info/RECORD", "")
    return filename
"#,
    )?;
    let builder_url = Url::from_directory_path(builder_dir.path()).unwrap();

    for (name, marker) in [
        ("alpha-parent", "sys_platform == 'linux'"),
        ("omega-parent", "sys_platform == 'win32'"),
    ] {
        let parent_dir = context.temp_dir.child(name);
        parent_dir.create_dir_all()?;
        parent_dir.child("pyproject.toml").write_str(&format!(
            r#"
            [project]
            name = "{name}"
            version = "0.1.0"
            requires-python = ">=3.12"

            [build-system]
            requires = ["builder @ {builder_url} ; {marker}"]
            backend-path = ["."]
            build-backend = "build_backend"
            "#
        ))?;
        parent_dir.child("build_backend.py").write_str(
            r#"
def get_requires_for_build_wheel(config_settings=None):
    return []
"#,
        )?;
    }

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "alpha-parent",
            "omega-parent",
        ]

        [tool.uv.sources]
        alpha-parent = { path = "alpha-parent" }
        omega-parent = { path = "omega-parent" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let builder = package_section(&lock, "builder");
    assert!(
        builder.contains(r#"{ name = "seed-linux", version = "0.1.0""#),
        "{builder}"
    );
    assert!(
        builder.contains(r#"{ name = "seed-win", version = "0.1.0""#),
        "{builder}"
    );

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

/// Verify build locking honors extra build dependencies constrained to the runtime resolution.
#[test]
fn lock_build_dependencies_extra_match_runtime() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let child_dir = context.temp_dir.child("child");
    child_dir.create_dir_all()?;
    child_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    child_dir.child("build_backend.py").write_str(
        r#"
def get_requires_for_build_wheel(config_settings=None):
    return []
"#,
    )?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["anyio<4.1", "child"]

        [tool.uv.sources]
        child = { path = "child" }

        [tool.uv.extra-build-dependencies]
        child = [{ requirement = "anyio", match-runtime = true }]
        "#,
    )?;

    context
        .lock()
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    let child = package_section(&lock, "child");
    assert!(
        child.contains(r#"{ name = "anyio", version = "4.0.0" }"#),
        "{child}"
    );
    assert!(
        child.contains(r#"build-requires = [{ name = "anyio" }]"#),
        "{child}"
    );

    context
        .lock()
        .arg("--preview-features")
        .arg("extra-build-dependencies,lock-build-dependencies")
        .arg("--locked")
        .assert()
        .success();

    Ok(())
}

/// Verify the hook-expanded build resolution replaces its initial stage without dropping extras.
#[test]
fn lock_build_dependencies_hook_resolution_replaces_initial_stage() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    for version in ["1.0.0", "2.0.0"] {
        write_wheel(
            &links_dir.child(format!("seed-{version}-py3-none-any.whl")),
            "seed",
            version,
        )?;
    }
    write_wheel(
        &links_dir.child("extra-0.1.0-py3-none-any.whl"),
        "extra",
        "0.1.0",
    )?;

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        dynamic = ["version"]
        requires-python = ">=3.12"

        [build-system]
        requires = ["seed"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    dep_dir.child("build_backend.py").write_str(
        r#"
from pathlib import Path

def get_requires_for_build_wheel(config_settings=None):
    return ["seed<2"]

def prepare_metadata_for_build_wheel(metadata_directory, config_settings=None):
    dist_info = Path(metadata_directory) / "dep-0.1.0.dist-info"
    dist_info.mkdir()
    (dist_info / "METADATA").write_text(
        "Metadata-Version: 2.3\nName: dep\nVersion: 0.1.0\n"
    )
    return dist_info.name
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

        [tool.uv.extra-build-dependencies]
        dep = ["extra"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
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
        dep.contains(r#"{ name = "extra", version = "0.1.0" }"#),
        "{dep}"
    );
    assert!(
        dep.contains(r#"{ name = "seed", version = "1.0.0" }"#),
        "{dep}"
    );
    assert!(
        !dep.contains(r#"{ name = "seed", version = "2.0.0" }"#),
        "{dep}"
    );

    Ok(())
}

/// Verify static metadata builds lock dependencies returned by the backend hook.
#[test]
fn lock_build_dependencies_static_metadata_captures_hook_requirements() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
        "0.1.0",
    )?;

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    dep_dir.child("build_backend.py").write_str(
        r#"
from importlib.metadata import PackageNotFoundError, version
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    try:
        version("helper")
    except PackageNotFoundError:
        return ["helper"]
    raise RuntimeError("helper is installed before the backend hook runs")

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    if version("helper") != "0.1.0":
        raise RuntimeError("helper is unavailable")
    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep/__init__.py", "")
        wheel.writestr(
            "dep-0.1.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: dep\nVersion: 0.1.0\n",
        )
        wheel.writestr(
            "dep-0.1.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("dep-0.1.0.dist-info/RECORD", "")
    return filename
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
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
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
    assert!(
        dep.contains(r#"{ name = "helper", version = "0.1.0" }"#),
        "{dep}"
    );

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + dep==0.1.0 (from file://[TEMP_DIR]/dep)
    ");

    Ok(())
}

/// Verify that frozen builds do not reuse locked dependencies after a backend hook changes them.
#[test]
fn lock_build_dependencies_static_metadata_revalidates_hook_requirements() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
        "0.1.0",
    )?;
    write_wheel(
        &links_dir.child("helper-0.2.0-py3-none-any.whl"),
        "helper",
        "0.2.0",
    )?;

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;

    let write_backend = |helper_version: &str| {
        dep_dir.child("build_backend.py").write_str(
            &r#"
from importlib.metadata import version
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return ["helper=={HELPER_VERSION}"]

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    if version("helper") != "{HELPER_VERSION}":
        raise RuntimeError("unexpected helper version")
    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep/__init__.py", "")
        wheel.writestr(
            "dep-0.1.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: dep\nVersion: 0.1.0\n",
        )
        wheel.writestr(
            "dep-0.1.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("dep-0.1.0.dist-info/RECORD", "")
    return filename
"#
            .replace("{HELPER_VERSION}", helper_version),
        )
    };
    write_backend("0.1.0")?;

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
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
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
    assert!(
        dep.contains(r#"{ name = "helper", version = "0.1.0" }"#),
        "{dep}"
    );

    write_backend("0.2.0")?;

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + dep==0.1.0 (from file://[TEMP_DIR]/dep)
    ");

    Ok(())
}

/// Verify that build-only dependency edges are retained for runtime-shared packages.
#[test]
fn lock_build_dependencies_merge_runtime_shared_build_edges() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let (build_marker, runtime_marker) = if cfg!(target_os = "macos") {
        ("sys_platform == 'darwin'", "sys_platform != 'darwin'")
    } else if cfg!(target_os = "windows") {
        ("sys_platform == 'win32'", "sys_platform != 'win32'")
    } else {
        ("sys_platform == 'linux'", "sys_platform != 'linux'")
    };

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    let leaf_requirement = format!("leaf==0.1.0 ; {build_marker}");
    write_wheel_with_requires(
        &links_dir.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
        "0.1.0",
        &[&leaf_requirement],
    )?;
    write_wheel(
        &links_dir.child("leaf-0.1.0-py3-none-any.whl"),
        "leaf",
        "0.1.0",
    )?;

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["helper==0.1.0 ; {build_marker}"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#
    ))?;
    dep_dir.child("build_backend.py").write_str(
        r#"
from importlib.metadata import version
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    if version("leaf") != "0.1.0":
        raise RuntimeError("leaf is unavailable")
    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep/__init__.py", "")
        wheel.writestr(
            "dep-0.1.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: dep\nVersion: 0.1.0\n",
        )
        wheel.writestr(
            "dep-0.1.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("dep-0.1.0.dist-info/RECORD", "")
    return filename
"#,
    )?;

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&format!(
            r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "dep",
            "helper==0.1.0 ; {runtime_marker}",
        ]

        [tool.uv.sources]
        dep = {{ path = "dep" }}
        "#
        ))?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + dep==0.1.0 (from file://[TEMP_DIR]/dep)
    ");

    Ok(())
}

/// Verify that build-required artifacts are retained for packages that are
/// selected by the runtime resolution only on another platform.
#[test]
fn lock_build_dependencies_merge_runtime_shared_build_artifacts() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let (host_wheel_tag, foreign_wheel_tag, foreign_marker) = if cfg!(target_os = "macos") {
        (
            "macosx_10_9_universal2",
            "win_amd64",
            "sys_platform == 'win32'",
        )
    } else if cfg!(target_os = "windows") {
        (
            if cfg!(target_arch = "aarch64") {
                "win_arm64"
            } else {
                "win_amd64"
            },
            "macosx_10_9_universal2",
            "sys_platform == 'darwin'",
        )
    } else {
        (
            if cfg!(target_arch = "aarch64") {
                "manylinux_2_17_aarch64.manylinux2014_aarch64"
            } else {
                "manylinux_2_17_x86_64.manylinux2014_x86_64"
            },
            "win_amd64",
            "sys_platform == 'win32'",
        )
    };

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child(format!("shared-0.1.0-py3-none-{host_wheel_tag}.whl")),
        "shared",
        "0.1.0",
    )?;
    write_wheel(
        &links_dir.child(format!("shared-0.1.0-py3-none-{foreign_wheel_tag}.whl")),
        "shared",
        "0.1.0",
    )?;

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["shared==0.1.0"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    dep_dir.child("build_backend.py").write_str(
        r#"
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep/__init__.py", "")
        wheel.writestr(
            "dep-0.1.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: dep\nVersion: 0.1.0\n",
        )
        wheel.writestr(
            "dep-0.1.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("dep-0.1.0.dist-info/RECORD", "")
    return filename
"#,
    )?;

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&format!(
            r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "dep",
            "shared==0.1.0 ; {foreign_marker}",
        ]

        [tool.uv.sources]
        dep = {{ path = "dep" }}
        "#
        ))?;

    context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--no-index")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + dep==0.1.0 (from file://[TEMP_DIR]/dep)
    ");

    Ok(())
}

/// Verify that source distributions selected for a cross-target sync retain
/// their locked build environment on the host performing the build.
#[test]
fn lock_build_dependencies_replay_cross_target_source_builds() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let (target_platform, target_marker) = if cfg!(target_os = "windows") {
        ("linux", "sys_platform == 'linux'")
    } else {
        ("windows", "sys_platform == 'win32'")
    };

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("builder-0.1.0-py3-none-any.whl"),
        "builder",
        "0.1.0",
    )?;

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["builder==0.1.0"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    dep_dir.child("build_backend.py").write_str(
        r#"
import builder
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep/__init__.py", "")
        wheel.writestr(
            "dep-0.1.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: dep\nVersion: 0.1.0\n",
        )
        wheel.writestr(
            "dep-0.1.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("dep-0.1.0.dist-info/RECORD", "")
    return filename
"#,
    )?;

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&format!(
            r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep ; {target_marker}"]

        [tool.uv.sources]
        dep = {{ path = "dep" }}
        "#
        ))?;

    context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--python-platform")
        .arg(target_platform)
        .arg("--no-index")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + dep==0.1.0 (from file://[TEMP_DIR]/dep)
    ");

    Ok(())
}

/// Verify that build dependency edges are scoped to each isolated source build
/// when the same build package resolves a transitive dependency differently.
#[test]
fn lock_build_dependencies_isolates_shared_build_dependency_edges() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel_with_requires(
        &links_dir.child("builder-1.0.0-py3-none-any.whl"),
        "builder",
        "1.0.0",
        &["helper>=1"],
    )?;
    write_wheel(
        &links_dir.child("helper-1.0.0-py3-none-any.whl"),
        "helper",
        "1.0.0",
    )?;
    write_wheel(
        &links_dir.child("helper-2.0.0-py3-none-any.whl"),
        "helper",
        "2.0.0",
    )?;

    for (name, helper_version) in [("dep-a", "1.0.0"), ("dep-b", "2.0.0")] {
        let module_name = name.replace('-', "_");
        let dep_dir = context.temp_dir.child(name);
        dep_dir.create_dir_all()?;
        dep_dir.child("pyproject.toml").write_str(&format!(
            r#"
            [project]
            name = "{name}"
            version = "0.1.0"
            requires-python = ">=3.12"

            [build-system]
            requires = ["builder==1.0.0", "helper=={helper_version}"]
            backend-path = ["."]
            build-backend = "build_backend"
            "#
        ))?;
        dep_dir.child("build_backend.py").write_str(&format!(
            r#"
from importlib.metadata import version
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    if version("helper") != "{helper_version}":
        raise RuntimeError("unexpected helper version: " + version("helper"))
    filename = "{module_name}-0.1.0-py3-none-any.whl"
    dist_info = "{module_name}-0.1.0.dist-info"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("{module_name}/__init__.py", "")
        wheel.writestr(
            dist_info + "/METADATA",
            "Metadata-Version: 2.3\nName: {name}\nVersion: 0.1.0\n",
        )
        wheel.writestr(
            dist_info + "/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr(dist_info + "/RECORD", "")
    return filename
"#
        ))?;
    }

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

    context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + dep-a==0.1.0 (from file://[TEMP_DIR]/dep-a)
     + dep-b==0.1.0 (from file://[TEMP_DIR]/dep-b)
    ");

    Ok(())
}

/// Verify that isolated build dependency edges do not affect the runtime
/// dependency closure when a package is shared between both environments.
#[test]
fn lock_build_dependencies_do_not_leak_edges_into_runtime_graph() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel_with_requires(
        &links_dir.child("builder-1.0.0-py3-none-any.whl"),
        "builder",
        "1.0.0",
        &["leaf>=1"],
    )?;
    write_wheel(
        &links_dir.child("leaf-1.0.0-py3-none-any.whl"),
        "leaf",
        "1.0.0",
    )?;
    write_wheel(
        &links_dir.child("leaf-2.0.0-py3-none-any.whl"),
        "leaf",
        "2.0.0",
    )?;

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["builder==1.0.0", "leaf==1.0.0"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    dep_dir.child("build_backend.py").write_str(
        r#"
from importlib.metadata import version
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    if version("leaf") != "1.0.0":
        raise RuntimeError("unexpected build leaf version: " + version("leaf"))
    filename = "dep-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("dep/__init__.py", "")
        wheel.writestr(
            "dep-0.1.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: dep\nVersion: 0.1.0\n",
        )
        wheel.writestr(
            "dep-0.1.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("dep-0.1.0.dist-info/RECORD", "")
    return filename
"#,
    )?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["builder==1.0.0", "dep"]

        [tool.uv.sources]
        dep = { path = "dep" }
        "#,
    )?;

    context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--frozen")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 2 packages in [TIME]
    Installed 3 packages in [TIME]
     + builder==1.0.0
     + dep==0.1.0 (from file://[TEMP_DIR]/dep)
     + leaf==2.0.0
    ");

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

/// Verify build dependency solves are restricted to a conditional source's Python region.
#[test]
fn lock_build_dependencies_use_conditional_source_python_range() -> Result<()> {
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
        requires-python = ">=3.8"
        dependencies = ["dep ; python_version >= '3.12'"]

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
    assert!(
        package_section(&lock, "dep").contains(
            r#"{ name = "builder", version = "0.1.0", marker = "python_full_version >= '3.12'" }"#
        ),
        "{lock}"
    );

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

/// Verify that target platform markers do not constrain cached implicit
/// backend build roots, which must be usable by the build host.
#[test]
fn lock_build_dependencies_default_backend_cache_ignores_target_platform() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let (host_marker, foreign_marker) = if cfg!(target_os = "macos") {
        ("sys_platform == 'darwin'", "sys_platform == 'linux'")
    } else if cfg!(target_os = "windows") {
        ("sys_platform == 'win32'", "sys_platform == 'linux'")
    } else {
        ("sys_platform == 'linux'", "sys_platform == 'darwin'")
    };

    for name in ["dep-a", "dep-b"] {
        let module_name = name.replace('-', "_");
        let archive_name = format!("{name}-0.1.0.zip");
        let source_dist = context.temp_dir.child(&archive_name);
        let file = File::create(source_dist.path())?;
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        zip.add_directory(format!("{name}-0.1.0/"), options)?;
        zip.start_file(format!("{name}-0.1.0/pyproject.toml"), options)?;
        zip.write_all(
            format!(
                r#"
                [project]
                name = "{name}"
                dynamic = ["version"]
                requires-python = ">=3.12"

                [tool.setuptools.dynamic]
                version = {{attr = "{module_name}.__version__"}}
                "#
            )
            .as_bytes(),
        )?;
        zip.add_directory(format!("{name}-0.1.0/{module_name}/"), options)?;
        zip.start_file(format!("{name}-0.1.0/{module_name}/__init__.py"), options)?;
        zip.write_all(b"__version__ = '0.1.0'")?;
        zip.finish()?;
    }

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&format!(
            r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "dep-a ; {host_marker}",
            "dep-b ; {foreign_marker}",
        ]

        [tool.uv.sources]
        dep-a = {{ path = "dep-a-0.1.0.zip" }}
        dep-b = {{ path = "dep-b-0.1.0.zip" }}
        "#
        ))?;

    context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    let dep_a = package_section(&lock, "dep-a");
    assert!(dep_a.contains(r#"{ name = "setuptools","#), "{dep_a}");
    assert!(!dep_a.contains("marker ="), "{dep_a}");
    let dep_b = package_section(&lock, "dep-b");
    assert!(dep_b.contains(r#"{ name = "setuptools","#), "{dep_b}");
    assert!(!dep_b.contains("marker ="), "{dep_b}");

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

/// Verify that an explicit empty build-system requirement set is not treated
/// as the implicit default backend environment.
#[test]
fn lock_build_dependencies_explicit_empty_build_requires_invalidate() -> Result<()> {
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
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    dep_dir.child("build_backend.py").write_str(
        r#"
def get_requires_for_build_wheel(config_settings=None):
    return []
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

    context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"
        "#,
    )?;

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

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(dep.contains("build-requires = []"), "{dep}");

    Ok(())
}

/// Verify that static directory dependencies lower build requirements through
/// their own source configuration before locking the build environment.
#[test]
fn lock_build_dependencies_static_directory_lowers_build_sources() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    write_wheel(
        &context
            .temp_dir
            .child("private_builder-0.1.0-py3-none-any.whl"),
        "private-builder",
        "0.1.0",
    )?;

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["private-builder"]
        build-backend = "private_builder"

        [tool.uv.sources]
        private-builder = { path = "../private_builder-0.1.0-py3-none-any.whl" }
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
        .arg("--no-index"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(
        dep.contains(r#"{ name = "private-builder", version = "0.1.0" }"#),
        "{dep}"
    );
    let private_builder = package_section(&lock, "private-builder");
    assert!(
        private_builder.contains(r#"hash = "sha256:"#),
        "{private_builder}"
    );

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index")
        .arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    Ok(())
}

/// Verify that changing only a static dependency's lowered build source
/// invalidates its locked build environment.
#[test]
fn lock_build_dependencies_static_directory_lowered_sources_invalidate() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    for version in ["0.1.0", "0.2.0"] {
        write_wheel(
            &context
                .temp_dir
                .child(format!("private_builder-{version}-py3-none-any.whl")),
            "private-builder",
            version,
        )?;
    }

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    let write_dep = |version: &str| -> Result<()> {
        dep_dir.child("pyproject.toml").write_str(&format!(
            r#"
            [project]
            name = "dep"
            version = "0.1.0"
            requires-python = ">=3.12"

            [build-system]
            requires = ["private-builder"]
            build-backend = "private_builder"

            [tool.uv.sources]
            private-builder = {{ path = "../private_builder-{version}-py3-none-any.whl" }}
            "#
        ))?;
        Ok(())
    };
    write_dep("0.1.0")?;

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
        .arg("--no-index"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    assert!(
        package_section(&context.read("uv.lock"), "dep")
            .contains(r#"{ name = "private-builder", version = "0.1.0" }"#)
    );

    write_dep("0.2.0")?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index")
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
        .arg("lock-build-dependencies")
        .arg("--no-index"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Updated private-builder v0.1.0 -> v0.2.0
    ");
    assert!(
        package_section(&context.read("uv.lock"), "dep")
            .contains(r#"{ name = "private-builder", version = "0.2.0" }"#)
    );

    Ok(())
}

/// Verify that static Git dependencies lower their declared build requirements
/// through their own source configuration before locking the build environment.
#[test]
#[cfg(feature = "test-git")]
fn lock_build_dependencies_static_git() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    write_wheel(
        &dep_dir.child("private_builder-0.1.0-py3-none-any.whl"),
        "private-builder",
        "0.1.0",
    )?;
    dep_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["private-builder"]
        build-backend = "private_builder"

        [tool.uv.sources]
        private-builder = { path = "private_builder-0.1.0-py3-none-any.whl" }
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
        .args(["add", "."])
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
        .arg("lock-build-dependencies")
        .arg("--no-index"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(dep.contains("build-dependencies = ["), "{dep}");
    assert!(
        dep.contains(r#"{ name = "private-builder", version = "0.1.0" }"#),
        "{dep}"
    );

    Ok(())
}

/// Verify that static archives stored in Git capture dependencies returned by
/// their backend hook before the build resolution is locked.
#[test]
#[cfg(feature = "test-git")]
fn lock_build_dependencies_static_git_archive_captures_hook_requirements() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
        "0.1.0",
    )?;

    let repo_dir = &context.temp_dir;
    repo_dir.child("archives").create_dir_all()?;
    let source_dist = repo_dir.child("archives/dep-0.1.0.zip");
    let file = File::create(source_dist.path())?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    zip.add_directory("dep-0.1.0/", options)?;
    zip.start_file("dep-0.1.0/pyproject.toml", options)?;
    zip.write_all(
        br#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    zip.start_file("dep-0.1.0/build_backend.py", options)?;
    zip.write_all(
        br#"
def get_requires_for_build_wheel(config_settings=None):
    return ["helper==0.1.0"]
        "#,
    )?;
    zip.finish()?;

    Command::new("git")
        .arg("init")
        .arg("--quiet")
        .arg(repo_dir.path())
        .assert()
        .success();
    Command::new("git")
        .args(["-C", repo_dir.path().to_str().expect("UTF-8 temp path")])
        .args(["config", "user.name", "uv-test"])
        .assert()
        .success();
    Command::new("git")
        .args(["-C", repo_dir.path().to_str().expect("UTF-8 temp path")])
        .args(["config", "user.email", "uv-test@example.com"])
        .assert()
        .success();
    Command::new("git")
        .args(["-C", repo_dir.path().to_str().expect("UTF-8 temp path")])
        .args(["add", "."])
        .assert()
        .success();
    Command::new("git")
        .args(["-C", repo_dir.path().to_str().expect("UTF-8 temp path")])
        .args(["commit", "--quiet", "-m", "initial"])
        .assert()
        .success();

    let repo_url = Url::from_directory_path(repo_dir.path()).expect("valid file URL");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&format!(
            r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["dep @ git+{repo_url}#path=archives/dep-0.1.0.zip"]
        "#
        ))?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
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
    assert!(
        dep.contains(r#"{ name = "helper", version = "0.1.0" }"#),
        "{dep}"
    );

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

/// Verify build dependencies are captured for the project itself.
#[test]
fn lock_build_dependencies_trivial_project() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42", "wheel"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;
    context.temp_dir.child("project").create_dir_all()?;
    context.temp_dir.child("project/__init__.py").touch()?;

    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");

    let lock = context.read("uv.lock");
    let project = package_section(&lock, "project");
    assert!(project.contains(r#"source = { editable = "." }"#));
    assert!(project.contains("build-dependencies = ["));
    assert!(project.contains(r#"{ name = "setuptools", version = "69.2.0" }"#));
    assert!(project.contains(r#"{ name = "wheel", version = "0.43.0" }"#));
    assert!(project.contains("[package.metadata]"));
    assert!(project.contains(r#"{ name = "setuptools", specifier = ">=42" }"#));
    assert!(project.contains(r#"{ name = "wheel" }"#));

    Ok(())
}

/// Verify virtual projects do not resolve declared build requirements, since they are not built.
#[test]
fn lock_build_dependencies_virtual_project_skips_build_requirements() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["unavailable-builder"]
        build-backend = "unavailable_builder"

        [tool.uv]
        package = false
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");

    let lock = context.read("uv.lock");
    let project = package_section(&lock, "project");
    assert!(project.contains(r#"source = { virtual = "." }"#));
    assert!(!project.contains("build-dependencies = ["));

    Ok(())
}

/// Verify build dependencies are captured for workspace members.
#[test]
fn lock_build_dependencies_workspace_member() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["member"]

        [tool.uv.sources]
        member = { workspace = true }

        [tool.uv.workspace]
        members = ["member"]
        "#,
    )?;

    let member_dir = context.temp_dir.child("member");
    member_dir.create_dir_all()?;
    member_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "member"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["setuptools>=42", "wheel"]
        build-backend = "setuptools.build_meta"
        "#,
    )?;
    member_dir.child("member").create_dir_all()?;
    member_dir.child("member/__init__.py").touch()?;

    uv_snapshot!(context.filters(), context.lock().arg("--preview-features").arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let member = package_section(&lock, "member");
    assert!(member.contains(r#"source = { editable = "member" }"#));
    assert!(member.contains("build-dependencies = ["));
    assert!(member.contains(r#"{ name = "setuptools", version = "69.2.0" }"#));
    assert!(member.contains(r#"{ name = "wheel", version = "0.43.0" }"#));
    assert!(member.contains("[package.metadata]"));
    assert!(member.contains(r#"{ name = "setuptools", specifier = ">=42" }"#));
    assert!(member.contains(r#"{ name = "wheel" }"#));

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

/// Verify that stale build requirements in non-host marker branches invalidate the lock file.
#[test]
fn lock_build_dependencies_stale_build_requires_foreign_platform() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let foreign_marker = if cfg!(target_os = "windows") {
        "sys_platform == 'linux'"
    } else {
        "sys_platform == 'win32'"
    };

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("seed-0.1.0-py3-none-any.whl"),
        "seed",
        "0.1.0",
    )?;
    write_wheel(
        &links_dir.child("seed-0.2.0-py3-none-any.whl"),
        "seed",
        "0.2.0",
    )?;

    let builder_dir = context.temp_dir.child("builder");
    builder_dir.create_dir_all()?;
    let builder_pyproject = builder_dir.child("pyproject.toml");
    let write_builder = |seed_version: &str| {
        builder_pyproject.write_str(&format!(
            r#"
        [project]
        name = "builder"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["seed=={seed_version}"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#
        ))
    };
    write_builder("0.1.0")?;
    builder_dir.child("build_backend.py").write_str(
        r#"
def get_requires_for_build_wheel(config_settings=None):
    return []
"#,
    )?;
    let builder_url = Url::from_directory_path(builder_dir.path()).unwrap();

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["builder @ {builder_url} ; {foreign_marker}"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#
    ))?;
    dep_dir.child("build_backend.py").write_str(
        r#"
def get_requires_for_build_wheel(config_settings=None):
    return []
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
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    assert!(
        package_section(&lock, "builder").contains(r#"{ name = "seed", version = "0.1.0","#),
        "{lock}"
    );

    write_builder("0.2.0")?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
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
        name = "anyio"
        version = "4.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "idna" },
            { name = "sniffio" },
        ]
        build-dependencies = [
            { name = "setuptools", version = "69.2.0", marker = "sys_platform == 'linux'" },
            { name = "setuptools-scm", version = "8.0.4", marker = "sys_platform == 'linux'" },
            { name = "wheel", version = "0.43.0", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642, upload-time = "2024-02-19T08:36:28.641Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584, upload-time = "2024-02-19T08:36:26.842Z" },
        ]

        [[package]]
        name = "calver"
        version = "2022.6.26"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "setuptools", version = "69.2.0", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "wheel", version = "0.43.0", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b5/00/96cbed7c019c49ee04b8a08357a981983db7698ae6de402e57097cefc9ad/calver-2022.6.26.tar.gz", hash = "sha256:e05493a3b17517ef1748fbe610da11f10485faa7c416b9d33fd4a52d74894f8b", size = 6670, upload-time = "2022-06-26T23:25:10.382Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f7/39/e421c06f42ca00fa9cf8929c2466e58a837e8e97b8ab3ff4f4ff9a15e33e/calver-2022.6.26-py3-none-any.whl", hash = "sha256:a1d7fcdd67797afc52ee36ffb8c8adf6643173864306547bfd1380cbce6310a0", size = 7049, upload-time = "2022-06-26T23:25:07.692Z" },
        ]

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
        name = "flit-core"
        version = "3.9.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/c4/e6/c1ac50fe3eebb38a155155711e6e864e254ce4b6e17fe2429b4c4d5b9e80/flit_core-3.9.0.tar.gz", hash = "sha256:72ad266176c4a3fcfab5f2930d76896059851240570ce9a98733b658cb786eba", size = 41917, upload-time = "2023-05-14T14:48:51.809Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/38/45/618e84e49a6c51e5dd15565ec2fcd82ab273434f236b8f108f065ded517a/flit_core-3.9.0-py3-none-any.whl", hash = "sha256:7aada352fb0c7f5538c4fafeddf314d3a6a92ee8e2b1de70482329e42de70301", size = 63141, upload-time = "2023-05-14T14:48:49.24Z" },
        ]

        [[package]]
        name = "hatch-vcs"
        version = "0.4.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "hatchling", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "setuptools-scm", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
        ]
        build-dependencies = [
            { name = "hatchling", version = "1.22.4", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/f5/c9/54bb4fa27b4e4a014ef3bb17710cdf692b3aa2cbc7953da885f1bf7e06ea/hatch_vcs-0.4.0.tar.gz", hash = "sha256:093810748fe01db0d451fabcf2c1ac2688caefd232d4ede967090b1c1b07d9f7", size = 10917, upload-time = "2023-11-06T06:24:57.228Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/82/0f/6cbd9976160bc334add63bc2e7a58b1433a31b34b7cda6c5de6dd983d9a7/hatch_vcs-0.4.0-py3-none-any.whl", hash = "sha256:b8a2b6bee54cf6f9fc93762db73890017ae59c9081d1038a41f16235ceaf8b2c", size = 8412, upload-time = "2023-11-06T06:24:55.389Z" },
        ]

        [[package]]
        name = "hatchling"
        version = "1.22.4"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "packaging", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "pathspec", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "pluggy", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "trove-classifiers", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
        ]
        build-dependencies = [
            { name = "packaging", version = "24.0", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "pathspec", version = "0.12.1", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "pluggy", version = "1.4.0", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "trove-classifiers", version = "2024.3.3", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/4f/2a/c34d71531d1e1c9a5029bb73eb3816285befd0fffd7c63ffa0544253dca8/hatchling-1.22.4.tar.gz", hash = "sha256:8a2dcec96d7fb848382ef5848e5ac43fdae641f35a08a3fab5116bd495f3416e", size = 62758, upload-time = "2024-03-24T02:00:59.122Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/63/2d56d6356f9f8b906aa68335cbf5b1b54c69873a2e271eda2ddba319c1ae/hatchling-1.22.4-py3-none-any.whl", hash = "sha256:f56da5bfc396af7b29daa3164851dd04991c994083f56cb054b5003675caecdc", size = 82032, upload-time = "2024-03-24T02:00:57.534Z" },
        ]

        [[package]]
        name = "idna"
        version = "3.6"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "flit-core", version = "3.9.0", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426, upload-time = "2023-11-25T15:40:54.902Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567, upload-time = "2023-11-25T15:40:52.604Z" },
        ]

        [[package]]
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "hatch-vcs", version = "0.4.0", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "hatchling", version = "1.22.4", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "flit-core", version = "3.9.0", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "flit-core", version = "3.9.0", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "pluggy"
        version = "1.4.0"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "setuptools", version = "69.2.0", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "setuptools-scm", version = "8.0.4", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "setuptools-scm", version = "8.0.4", extra = ["toml"], marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "wheel", version = "0.43.0", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/54/c6/43f9d44d92aed815e781ca25ba8c174257e27253a94630d21be8725a2b59/pluggy-1.4.0.tar.gz", hash = "sha256:8c85c2876142a764e5b7548e7d9a0e0ddb46f5185161049a79b7e974454223be", size = 65812, upload-time = "2024-01-24T13:45:15.875Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a5/5b/0cc789b59e8cc1bf288b38111d002d8c5917123194d45b29dcdac64723cc/pluggy-1.4.0-py3-none-any.whl", hash = "sha256:7db9f7b503d67d1c5b95f59773ebb58a8c1c288129a88665838012cfb07b8981", size = 20120, upload-time = "2024-01-24T13:45:14.227Z" },
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
        name = "setuptools-scm"
        version = "8.0.4"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "packaging", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "packaging", marker = "sys_platform == 'linux'" },
            { name = "setuptools", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "setuptools", marker = "sys_platform == 'linux'" },
            { name = "typing-extensions", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "typing-extensions", marker = "sys_platform == 'linux'" },
        ]
        build-dependencies = [
            { name = "setuptools", version = "69.2.0", marker = "sys_platform == 'linux'" },
            { name = "wheel", version = "0.43.0", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/eb/b1/0248705f10f6de5eefe7ff93e399f7192257b23df4d431d2f5680bb2778f/setuptools-scm-8.0.4.tar.gz", hash = "sha256:b5f43ff6800669595193fd09891564ee9d1d7dcb196cab4b2506d53a2e1c95c7", size = 74280, upload-time = "2023-10-02T15:14:32.996Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/0e/a3/b9a8b0adfe672bf0df5901707aa929d30a97ee390ba651910186776746d2/setuptools_scm-8.0.4-py3-none-any.whl", hash = "sha256:b47844cd2a84b83b3187a5782c71128c28b4c94cad8bfb871da2784a5cb54c4f", size = 42137, upload-time = "2023-10-02T15:14:31.281Z" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "setuptools", version = "69.2.0", marker = "sys_platform == 'linux'" },
            { name = "setuptools-scm", version = "8.0.4", marker = "sys_platform == 'linux'" },
            { name = "wheel", version = "0.43.0", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372, upload-time = "2024-02-25T23:20:04.057Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235, upload-time = "2024-02-25T23:20:01.196Z" },
        ]

        [[package]]
        name = "trove-classifiers"
        version = "2024.3.3"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "calver", version = "2022.6.26", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "setuptools", version = "69.2.0", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
            { name = "wheel", version = "0.43.0", marker = "sys_platform == 'darwin' or sys_platform == 'win32'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/13/11/e13906315b498cb8f5ce5a7ff39fc35941e8291e914158157937fd1c095d/trove-classifiers-2024.3.3.tar.gz", hash = "sha256:df7edff9c67ff86b733628998330b180e81d125b1e096536d83ac0fd79673fdc", size = 15982, upload-time = "2024-03-03T20:17:38.634Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/bb/81/a16cb58f719e68d0cce72fb9afd6f0f50c0e474d7b8dc267c8309c3e2793/trove_classifiers-2024.3.3-py3-none-any.whl", hash = "sha256:3a84096861b385ec422c79995d1f6435dde47a9b63adaa3c886e53232ba7e6e0", size = 13377, upload-time = "2024-03-03T20:17:34.101Z" },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.10.0"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "flit-core", version = "3.9.0", marker = "sys_platform == 'linux'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/16/3a/0d26ce356c7465a19c9ea8814b960f8a36c3b0d07c323176620b7b483e44/typing_extensions-4.10.0.tar.gz", hash = "sha256:b0abd7c89e8fb96f98db18d86106ff1d90ab692004eb746cf6eda2682f91b3cb", size = 77558, upload-time = "2024-02-25T22:12:49.693Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f9/de/dc04a3ea60b22624b51c703a84bbe0184abcd1d0b9bc8074b5d6b7ab90bb/typing_extensions-4.10.0-py3-none-any.whl", hash = "sha256:69b1a937c3a517342112fb4c6df7e72fc39a38e7891a5730ed4985b5214b5475", size = 33926, upload-time = "2024-02-25T22:12:47.72Z" },
        ]

        [[package]]
        name = "wheel"
        version = "0.43.0"
        source = { registry = "https://pypi.org/simple" }
        build-dependencies = [
            { name = "flit-core", version = "3.9.0" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b8/d6/ac9cd92ea2ad502ff7c1ab683806a9deb34711a1e2bd8a59814e8fc27e69/wheel-0.43.0.tar.gz", hash = "sha256:465ef92c69fa5c5da2d1cf8ac40559a8c940886afcef87dcf14b9470862f1d85", size = 99109, upload-time = "2024-03-11T19:29:17.32Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/7d/cd/d7460c9a869b16c3dd4e1e403cce337df165368c71d6af229a74699622ce/wheel-0.43.0-py3-none-any.whl", hash = "sha256:55c570405f142630c6b9f72fe09d9b67cf1477fcf543ae5b8dcb1f5b7377da81", size = 65775, upload-time = "2024-03-11T19:29:15.522Z" },
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
    Resolved 18 packages in [TIME]
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

/// Verify a revision-3 lock remains reusable when all builds are disabled.
#[tokio::test]
async fn lock_build_dependencies_no_build_reuses_revision_3() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let artifacts = context.temp_dir.child("artifacts");
    artifacts.create_dir_all()?;
    write_wheel(
        &artifacts.child("runtime-0.1.0-py3-none-any.whl"),
        "runtime",
        "0.1.0",
    )?;

    let server = MockServer::start().await;
    let index_url = format!("{}/simple/", server.uri());
    Mock::given(method("GET"))
        .and(path("/simple/runtime/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!(
                r#"<a href="{}/files/runtime-0.1.0-py3-none-any.whl" data-upload-time="2024-03-01T00:00:00Z">runtime-0.1.0-py3-none-any.whl</a>"#,
                server.uri()
            ),
            "text/html",
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/runtime-0.1.0-py3-none-any.whl"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(fs_err::read(
            artifacts.child("runtime-0.1.0-py3-none-any.whl").path(),
        )?))
        .mount(&server)
        .await;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["runtime==0.1.0"]
        "#,
    )?;

    context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-build")
        .arg("--index-url")
        .arg(&index_url)
        .assert()
        .success();

    let lock = context.read("uv.lock");
    assert!(lock.contains("revision = 3"), "{lock}");
    assert!(!lock.contains("build-dependencies = ["), "{lock}");

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-build")
        .arg("--index-url")
        .arg(&index_url)
        .arg("--locked")
        .arg("--offline")
        .arg("--no-cache"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

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

/// Verify that compatible uv_build archives do not resolve an unused build environment.
#[test]
fn lock_build_dependencies_direct_build_static_sdist() -> Result<()> {
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
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
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
        .arg("lock-build-dependencies")
        .arg("--no-index"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(!dep.contains("build-dependencies = ["), "{dep}");

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index")
        .arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    Ok(())
}

/// Verify that static source archives lower build requirements through source configuration.
#[test]
fn lock_build_dependencies_static_sdist_lowers_build_sources() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let private_builder = context
        .temp_dir
        .child("private_builder-0.1.0-py3-none-any.whl");
    write_wheel(&private_builder, "private-builder", "0.1.0")?;

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
        requires = ["private-builder"]
        build-backend = "private_builder"

        [tool.uv.sources]
        private-builder = { path = "private_builder-0.1.0-py3-none-any.whl" }
        "#,
    ))?;
    let entry = ZipEntryBuilder::new(
        "dep-0.1.0/private_builder-0.1.0-py3-none-any.whl".into(),
        Compression::Stored,
    );
    block_on(zip.write_entry_whole(entry, &fs_err::read(private_builder.path())?))?;
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
        .arg("lock-build-dependencies")
        .arg("--no-index"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(
        dep.contains(r#"{ name = "private-builder", version = "0.1.0" }"#),
        "{dep}"
    );

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index")
        .arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");

    Ok(())
}

/// Verify that static source archives without an explicit build system lock
/// PEP 517's implicit setuptools build requirement.
#[test]
fn lock_build_dependencies_static_sdist_implicit_default_backend() -> Result<()> {
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

/// Verify that a complete empty build resolution is recorded and reused.
#[test]
fn lock_build_dependencies_empty_conditional_resolution() -> Result<()> {
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
        requires = ["never ; python_version < '3.0'"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    ))?;
    let entry = ZipEntryBuilder::new("dep-0.1.0/build_backend.py".into(), Compression::Stored);
    block_on(zip.write_entry_whole(
        entry,
        br#"
def get_requires_for_build_wheel(config_settings=None):
    return []
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
        .arg("lock-build-dependencies")
        .arg("--no-index"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    let dep = package_section(&lock, "dep");
    assert!(dep.contains("build-dependencies = []"), "{dep}");

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--no-index")
        .arg("--locked"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
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
            { name = "wheel", version = "0.43.0" },
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
        requires = ["setuptools>=42", "helper @ {helper_url}"]
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

/// Verify that a selected wheel does not reconstruct its locked source build
/// environment during frozen sync.
#[tokio::test]
async fn sync_filters_locked_build_resolutions_to_selected_wheels() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let builder_dir = context.temp_dir.child("builder");
    builder_dir.create_dir_all()?;
    builder_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "builder"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        build-backend = "builder"
        "#,
    )?;
    let builder_url = Url::from_directory_path(builder_dir.path()).expect("valid file URL");

    let artifacts = context.temp_dir.child("artifacts");
    artifacts.create_dir_all()?;
    write_wheel(
        &artifacts.child("wheel_selected_dep-0.1.0-py3-none-any.whl"),
        "wheel-selected-dep",
        "0.1.0",
    )?;

    let source_dist = artifacts.child("wheel_selected_dep-0.1.0.zip");
    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new(
        "wheel_selected_dep-0.1.0/pyproject.toml".into(),
        Compression::Stored,
    );
    let pyproject_toml = format!(
        r#"
            [project]
            name = "wheel-selected-dep"
            version = "0.1.0"
            requires-python = ">=3.12"

            [build-system]
            requires = ["builder @ {builder_url}"]
            build-backend = "builder"
            "#
    );
    block_on(zip.write_entry_whole(entry, pyproject_toml.as_bytes()))?;
    fs_err::write(source_dist.path(), block_on(zip.close())?)?;

    let server = MockServer::start().await;
    let index_url = format!("{}/simple/", server.uri());
    Mock::given(method("GET"))
        .and(path("/simple/wheel-selected-dep/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!(
                r#"
                <a href="{}/files/wheel_selected_dep-0.1.0-py3-none-any.whl" data-upload-time="2024-03-01T00:00:00Z">wheel_selected_dep-0.1.0-py3-none-any.whl</a>
                <a href="{}/files/wheel_selected_dep-0.1.0.zip" data-upload-time="2024-03-01T00:00:00Z">wheel_selected_dep-0.1.0.zip</a>
                "#,
                server.uri(),
                server.uri()
            ),
            "text/html",
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/wheel_selected_dep-0.1.0-py3-none-any.whl"))
        .respond_with(
            ResponseTemplate::new(200).set_body_bytes(fs_err::read(
                artifacts
                    .child("wheel_selected_dep-0.1.0-py3-none-any.whl")
                    .path(),
            )?),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/wheel_selected_dep-0.1.0.zip"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(fs_err::read(
            artifacts.child("wheel_selected_dep-0.1.0.zip").path(),
        )?))
        .mount(&server)
        .await;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["wheel-selected-dep"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--index-url")
        .arg(&index_url), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    assert!(
        package_section(&lock, "wheel-selected-dep").contains("build-dependencies = ["),
        "{lock}"
    );

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--frozen")
        .arg("--no-build")
        .arg("--index-url")
        .arg(index_url), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + wheel-selected-dep==0.1.0
    ");

    Ok(())
}

/// Verify that a wheel selected inside a locked build environment does not
/// reconstruct the build environment of its fallback source distribution.
#[tokio::test]
async fn sync_filters_nested_locked_build_resolutions_to_selected_wheels() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let backend = |name: &str| {
        format!(
            r#"
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "{name}-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("{name}/__init__.py", "")
        wheel.writestr(
            "{name}-0.1.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: {name}\nVersion: 0.1.0\n",
        )
        wheel.writestr(
            "{name}-0.1.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("{name}-0.1.0.dist-info/RECORD", "")
    return filename
"#
        )
    };

    let trouble_dir = context.temp_dir.child("trouble");
    trouble_dir.create_dir_all()?;
    trouble_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "trouble"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    trouble_dir
        .child("build_backend.py")
        .write_str(&backend("trouble"))?;
    let trouble_url = Url::from_directory_path(trouble_dir.path()).expect("valid file URL");

    let artifacts = context.temp_dir.child("artifacts");
    artifacts.create_dir_all()?;
    write_wheel(
        &artifacts.child("nested-0.1.0-py3-none-any.whl"),
        "nested",
        "0.1.0",
    )?;

    let nested_source_dist = artifacts.child("nested-0.1.0.zip");
    let mut zip = ZipFileWriter::new(Vec::new());
    let entry = ZipEntryBuilder::new("nested-0.1.0/pyproject.toml".into(), Compression::Stored);
    let pyproject_toml = format!(
        r#"
            [project]
            name = "nested"
            version = "0.1.0"
            requires-python = ">=3.12"

            [build-system]
            requires = ["trouble @ {trouble_url}"]
            backend-path = ["."]
            build-backend = "build_backend"
            "#
    );
    block_on(zip.write_entry_whole(entry, pyproject_toml.as_bytes()))?;
    let entry = ZipEntryBuilder::new("nested-0.1.0/build_backend.py".into(), Compression::Stored);
    let nested_backend = backend("nested");
    block_on(zip.write_entry_whole(entry, nested_backend.as_bytes()))?;
    fs_err::write(nested_source_dist.path(), block_on(zip.close())?)?;

    let server = MockServer::start().await;
    let index_url = format!("{}/simple/", server.uri());
    Mock::given(method("GET"))
        .and(path("/simple/nested/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!(
                r#"
                <a href="{}/files/nested-0.1.0-py3-none-any.whl" data-upload-time="2024-03-01T00:00:00Z">nested-0.1.0-py3-none-any.whl</a>
                <a href="{}/files/nested-0.1.0.zip" data-upload-time="2024-03-01T00:00:00Z">nested-0.1.0.zip</a>
                "#,
                server.uri(),
                server.uri()
            ),
            "text/html",
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/nested-0.1.0-py3-none-any.whl"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(fs_err::read(
            artifacts.child("nested-0.1.0-py3-none-any.whl").path(),
        )?))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/nested-0.1.0.zip"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(fs_err::read(artifacts.child("nested-0.1.0.zip").path())?),
        )
        .mount(&server)
        .await;

    let parent_dir = context.temp_dir.child("parent");
    parent_dir.create_dir_all()?;
    parent_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "parent"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["nested==0.1.0"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    parent_dir
        .child("build_backend.py")
        .write_str(&backend("parent"))?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["parent"]

        [tool.uv.sources]
        parent = { path = "parent" }
        "#,
    )?;

    uv_snapshot!(context.filters(), context
        .lock()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--index-url")
        .arg(&index_url), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let lock = context.read("uv.lock");
    assert!(
        package_section(&lock, "nested").contains("build-dependencies = ["),
        "{lock}"
    );

    uv_snapshot!(context.filters(), context
        .sync()
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--frozen")
        .arg("--no-build-package")
        .arg("trouble")
        .arg("--index-url")
        .arg(index_url), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + parent==0.1.0 (from file://[TEMP_DIR]/parent)
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

/// Verify that runtime metadata is preserved for source packages reached only
/// through build dependencies.
#[test]
fn lock_build_dependencies_build_only_source_package_metadata() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let links_dir = context.temp_dir.child("links");
    links_dir.create_dir_all()?;
    write_wheel(
        &links_dir.child("helper-0.1.0-py3-none-any.whl"),
        "helper",
        "0.1.0",
    )?;

    let builder_dir = context.temp_dir.child("builder");
    builder_dir.create_dir_all()?;
    builder_dir.child("pyproject.toml").write_str(
        r#"
        [project]
        name = "builder"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["helper==0.1.0"]

        [build-system]
        requires = []
        backend-path = ["."]
        build-backend = "build_backend"
        "#,
    )?;
    builder_dir.child("build_backend.py").write_str(
        r#"
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    return []

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    filename = "builder-0.1.0-py3-none-any.whl"
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr("builder/__init__.py", "")
        wheel.writestr(
            "builder-0.1.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: builder\nVersion: 0.1.0\nRequires-Dist: helper==0.1.0\n",
        )
        wheel.writestr(
            "builder-0.1.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        wheel.writestr("builder-0.1.0.dist-info/RECORD", "")
    return filename
"#,
    )?;
    let builder_url = Url::from_directory_path(builder_dir.path()).unwrap();

    let dep_dir = context.temp_dir.child("dep");
    dep_dir.create_dir_all()?;
    dep_dir.child("pyproject.toml").write_str(&format!(
        r#"
        [project]
        name = "dep"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["builder @ {builder_url}"]
        backend-path = ["."]
        build-backend = "build_backend"
        "#
    ))?;
    dep_dir.child("build_backend.py").write_str(
        r#"
def get_requires_for_build_wheel(config_settings=None):
    return []
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

    context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .assert()
        .success();

    let lock = context.read("uv.lock");
    let builder = package_section(&lock, "builder");
    assert!(builder.contains(r#"{ name = "helper" }"#), "{builder}");
    assert!(
        builder.contains(r#"requires-dist = [{ name = "helper", specifier = "==0.1.0" }]"#),
        "{builder}"
    );

    context
        .lock()
        .arg("--find-links")
        .arg(links_dir.path())
        .arg("--no-index")
        .arg("--preview-features")
        .arg("lock-build-dependencies")
        .arg("--locked")
        .assert()
        .success();

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

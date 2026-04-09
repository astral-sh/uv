#[cfg(any(feature = "test-git", feature = "test-git-lfs"))]
use std::collections::BTreeSet;
use std::process::Command;

use anyhow::{Context, Result};
use assert_cmd::assert::OutputAssertExt;
use assert_fs::{
    assert::PathAssert,
    fixture::{FileTouch, FileWriteStr, PathChild, PathCreateDir},
};
use indoc::indoc;
use insta::assert_snapshot;
use predicates::prelude::predicate;
use uv_fs::copy_dir_all;
use uv_static::EnvVars;

use uv_test::uv_snapshot;

#[test]
fn tool_install() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix()
        .with_filtered_compiled();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    tool_dir.child("black").assert(predicate::path::is_dir());
    tool_dir
        .child("black")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("black{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run black in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r#"
        #![TEMP_DIR]/tools/black/bin/python
        # -*- coding: utf-8 -*-
        import sys
        from black import patched_main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(patched_main())
        "#);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "black" }]

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
        sdist = { url = "https://files.pythonhosted.org/packages/8f/5f/bac24a952668c7482cfdb4ebf91ba57a796c9da8829363a772040c1a3312/black-24.3.0.tar.gz", hash = "sha256:a0c9c4a0771afc6919578cec71ce82a3e31e054904e7197deacbc9382671c41f", size = 634292, upload-time = "2024-03-15T19:35:43.699Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b6/c6/1d174efa9ff02b22d0124c73fc5f4d4fb006d0d9a081aadc354d05754a13/black-24.3.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:2818cf72dfd5d289e48f37ccfa08b460bf469e67fb7c4abb07edc2e9f16fb63f", size = 1600822, upload-time = "2024-03-15T19:45:20.337Z" },
            { url = "https://files.pythonhosted.org/packages/d9/ed/704731afffe460b8ff0672623b40fce9fe569f2ee617c15857e4d4440a3a/black-24.3.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:4acf672def7eb1725f41f38bf6bf425c8237248bb0804faa3965c036f7672d11", size = 1429987, upload-time = "2024-03-15T19:45:00.637Z" },
            { url = "https://files.pythonhosted.org/packages/a8/05/8dd038e30caadab7120176d4bc109b7ca2f4457f12eef746b0560a583458/black-24.3.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:c7ed6668cbbfcd231fa0dc1b137d3e40c04c7f786e626b405c62bcd5db5857e4", size = 1755319, upload-time = "2024-03-15T19:38:24.009Z" },
            { url = "https://files.pythonhosted.org/packages/71/9d/e5fa1ff4ef1940be15a64883c0bb8d2fcf626efec996eab4ae5a8c691d2c/black-24.3.0-cp312-cp312-win_amd64.whl", hash = "sha256:56f52cfbd3dabe2798d76dbdd299faa046a901041faf2cf33288bc4e6dae57b5", size = 1385180, upload-time = "2024-03-15T19:39:37.014Z" },
            { url = "https://files.pythonhosted.org/packages/4d/ea/31770a7e49f3eedfd8cd7b35e78b3a3aaad860400f8673994bc988318135/black-24.3.0-py3-none-any.whl", hash = "sha256:41622020d7120e01d377f74249e677039d20e6344ff5851de8a10f11f513bf93", size = 201493, upload-time = "2024-03-15T19:35:41.572Z" },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121, upload-time = "2023-08-17T17:29:11.868Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941, upload-time = "2023-08-17T17:29:10.08Z" },
        ]

        [[package]]
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433, upload-time = "2023-02-04T12:11:27.157Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695, upload-time = "2023-02-04T12:11:25.002Z" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055, upload-time = "2024-01-31T01:00:36.02Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717, upload-time = "2024-01-31T01:00:34.019Z" },
        ]

        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    uv_snapshot!(context.filters(), Command::new("black").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.3.0 (compiled: [COMPILED])
    Python (CPython) 3.12.[X]

    ----- stderr -----
    ");

    // Install another tool
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("flask")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    Installed 1 executable: flask
    ");

    tool_dir.child("flask").assert(predicate::path::is_dir());
    assert!(
        bin_dir
            .child(format!("flask{}", std::env::consts::EXE_SUFFIX))
            .exists()
    );

    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(fs_err::read_to_string(bin_dir.join("flask")).unwrap(), @r#"
        #![TEMP_DIR]/tools/flask/bin/python
        # -*- coding: utf-8 -*-
        import sys
        from flask.cli import main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(main())
        "#);
    });

    uv_snapshot!(context.filters(), Command::new("flask").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.2
    Werkzeug 3.0.1

    ----- stderr -----
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("flask").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "flask" }]

        [[package]]
        name = "blinker"
        version = "1.7.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a1/13/6df5fc090ff4e5d246baf1f45fe9e5623aa8565757dfa5bd243f6a545f9e/blinker-1.7.0.tar.gz", hash = "sha256:e6820ff6fa4e4d1d8e2747c2283749c3f547e4fee112b98555cdcdae32996182", size = 28134, upload-time = "2023-11-01T22:06:01.588Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/fa/2a/7f3714cbc6356a0efec525ce7a0613d581072ed6eb53eb7b9754f33db807/blinker-1.7.0-py3-none-any.whl", hash = "sha256:c3f865d4d54db7abc53758a01601cf343fe55b84c1de4e3fa910e420b438d5b9", size = 13068, upload-time = "2023-11-01T22:06:00.162Z" },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121, upload-time = "2023-08-17T17:29:11.868Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941, upload-time = "2023-08-17T17:29:10.08Z" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/3f/e0/a89e8120faea1edbfca1a9b171cff7f2bf62ec860bbafcb2c2387c0317be/flask-3.0.2.tar.gz", hash = "sha256:822c03f4b799204250a7ee84b1eddc40665395333973dfb9deebfe425fefcb7d", size = 675248, upload-time = "2024-02-03T21:11:44.79Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/93/a6/aa98bfe0eb9b8b15d36cdfd03c8ca86a03968a87f27ce224fb4f766acb23/flask-3.0.2-py3-none-any.whl", hash = "sha256:3232e0e9c850d781933cf0207523d1ece087eb8d87b23777ae38456e2fbe7c6e", size = 101300, upload-time = "2024-02-03T21:11:42.661Z" },
        ]

        [[package]]
        name = "itsdangerous"
        version = "2.1.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/7f/a1/d3fb83e7a61fa0c0d3d08ad0a94ddbeff3731c05212617dff3a94e097f08/itsdangerous-2.1.2.tar.gz", hash = "sha256:5dbbc68b317e5e42f327f9021763545dc3fc3bfe22e6deb96aaf1fc38874156a", size = 56143, upload-time = "2022-03-24T15:12:15.102Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/5f/447e04e828f47465eeab35b5d408b7ebaaaee207f48b7136c5a7267a30ae/itsdangerous-2.1.2-py3-none-any.whl", hash = "sha256:2c2349112351b88699d8d4b6b075022c0808887cb7ad10069318a8b0bc88db44", size = 15749, upload-time = "2022-03-24T15:12:13.2Z" },
        ]

        [[package]]
        name = "jinja2"
        version = "3.1.3"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "markupsafe" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b2/5e/3a21abf3cd467d7876045335e681d276ac32492febe6d98ad89562d1a7e1/Jinja2-3.1.3.tar.gz", hash = "sha256:ac8bd6544d4bb2c9792bf3a159e80bba8fda7f07e81bc3aed565432d5925ba90", size = 268261, upload-time = "2024-01-10T23:12:21.133Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/30/6d/6de6be2d02603ab56e72997708809e8a5b0fbfee080735109b40a3564843/Jinja2-3.1.3-py3-none-any.whl", hash = "sha256:7d6d50dd97d52cbc355597bd845fabfbac3f551e1f99619e39a35ce8c370b5fa", size = 133236, upload-time = "2024-01-10T23:12:19.504Z" },
        ]

        [[package]]
        name = "markupsafe"
        version = "2.1.5"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/87/5b/aae44c6655f3801e81aa3eef09dbbf012431987ba564d7231722f68df02d/MarkupSafe-2.1.5.tar.gz", hash = "sha256:d283d37a890ba4c1ae73ffadf8046435c76e7bc2247bbb63c00bd1a709c6544b", size = 19384, upload-time = "2024-02-02T16:31:22.863Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/53/bd/583bf3e4c8d6a321938c13f49d44024dbe5ed63e0a7ba127e454a66da974/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:8dec4936e9c3100156f8a2dc89c4b88d5c435175ff03413b443469c7c8c5f4d1", size = 18215, upload-time = "2024-02-02T16:30:33.081Z" },
            { url = "https://files.pythonhosted.org/packages/48/d6/e7cd795fc710292c3af3a06d80868ce4b02bfbbf370b7cee11d282815a2a/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:3c6b973f22eb18a789b1460b4b91bf04ae3f0c4234a0a6aa6b0a92f6f7b951d4", size = 14069, upload-time = "2024-02-02T16:30:34.148Z" },
            { url = "https://files.pythonhosted.org/packages/51/b5/5d8ec796e2a08fc814a2c7d2584b55f889a55cf17dd1a90f2beb70744e5c/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ac07bad82163452a6884fe8fa0963fb98c2346ba78d779ec06bd7a6262132aee", size = 29452, upload-time = "2024-02-02T16:30:35.149Z" },
            { url = "https://files.pythonhosted.org/packages/0a/0d/2454f072fae3b5a137c119abf15465d1771319dfe9e4acbb31722a0fff91/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f5dfb42c4604dddc8e4305050aa6deb084540643ed5804d7455b5df8fe16f5e5", size = 28462, upload-time = "2024-02-02T16:30:36.166Z" },
            { url = "https://files.pythonhosted.org/packages/2d/75/fd6cb2e68780f72d47e6671840ca517bda5ef663d30ada7616b0462ad1e3/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ea3d8a3d18833cf4304cd2fc9cbb1efe188ca9b5efef2bdac7adc20594a0e46b", size = 27869, upload-time = "2024-02-02T16:30:37.834Z" },
            { url = "https://files.pythonhosted.org/packages/b0/81/147c477391c2750e8fc7705829f7351cf1cd3be64406edcf900dc633feb2/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:d050b3361367a06d752db6ead6e7edeb0009be66bc3bae0ee9d97fb326badc2a", size = 33906, upload-time = "2024-02-02T16:30:39.366Z" },
            { url = "https://files.pythonhosted.org/packages/8b/ff/9a52b71839d7a256b563e85d11050e307121000dcebc97df120176b3ad93/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:bec0a414d016ac1a18862a519e54b2fd0fc8bbfd6890376898a6c0891dd82e9f", size = 32296, upload-time = "2024-02-02T16:30:40.413Z" },
            { url = "https://files.pythonhosted.org/packages/88/07/2dc76aa51b481eb96a4c3198894f38b480490e834479611a4053fbf08623/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:58c98fee265677f63a4385256a6d7683ab1832f3ddd1e66fe948d5880c21a169", size = 33038, upload-time = "2024-02-02T16:30:42.243Z" },
            { url = "https://files.pythonhosted.org/packages/96/0c/620c1fb3661858c0e37eb3cbffd8c6f732a67cd97296f725789679801b31/MarkupSafe-2.1.5-cp312-cp312-win32.whl", hash = "sha256:8590b4ae07a35970728874632fed7bd57b26b0102df2d2b233b6d9d82f6c62ad", size = 16572, upload-time = "2024-02-02T16:30:43.326Z" },
            { url = "https://files.pythonhosted.org/packages/3f/14/c3554d512d5f9100a95e737502f4a2323a1959f6d0d01e0d0997b35f7b10/MarkupSafe-2.1.5-cp312-cp312-win_amd64.whl", hash = "sha256:823b65d8706e32ad2df51ed89496147a42a2a6e01c13cfb6ffb8b1e92bc910bb", size = 17127, upload-time = "2024-02-02T16:30:44.418Z" },
        ]

        [[package]]
        name = "werkzeug"
        version = "3.0.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "markupsafe" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/0d/cc/ff1904eb5eb4b455e442834dabf9427331ac0fa02853bf83db817a7dd53d/werkzeug-3.0.1.tar.gz", hash = "sha256:507e811ecea72b18a404947aded4b3390e1db8f826b494d76550ef45bb3b1dcc", size = 801436, upload-time = "2023-10-24T20:57:50.084Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c3/fc/254c3e9b5feb89ff5b9076a23218dafbc99c96ac5941e900b71206e6313b/werkzeug-3.0.1-py3-none-any.whl", hash = "sha256:90a285dc0e42ad56b34e696398b8122ee4c681833fb35b8334a095d82c56da10", size = 226669, upload-time = "2023-10-24T20:57:47.326Z" },
        ]

        [tool]
        requirements = [{ name = "flask" }]
        entrypoints = [
            { name = "flask", install-path = "[TEMP_DIR]/bin/flask", from = "flask" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });
}

#[test]
fn tool_install_relative_exclude_newer_receipt_preserves_span() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    context
        .tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("black==24.2.0")
        .arg("--exclude-newer")
        .arg("3 weeks")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2024-05-01T00:00:00Z")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str())
        .assert()
        .success();

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-04-10T00:00:00Z"
        exclude-newer-span = "P3W"

        [manifest]
        requirements = [{ name = "black", specifier = "==24.2.0" }]

        [[package]]
        name = "black"
        version = "24.2.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "click" },
            { name = "mypy-extensions" },
            { name = "packaging" },
            { name = "pathspec" },
            { name = "platformdirs" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/29/69/f3ab49cdb938b3eecb048fa64f86bdadb1fac26e92c435d287181d543b0a/black-24.2.0.tar.gz", hash = "sha256:bce4f25c27c3435e4dace4815bcb2008b87e167e3bf4ee47ccdc5ce906eb4894", size = 631598, upload-time = "2024-02-12T20:21:26.969Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/43/1e/67c87a1fb39592aa944f35cc26892946ebe0a10aa324b87f9380b8753862/black-24.2.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:d84f29eb3ee44859052073b7636533ec995bd0f64e2fb43aeceefc70090e752b", size = 1585288, upload-time = "2024-02-12T20:37:13.8Z" },
            { url = "https://files.pythonhosted.org/packages/5e/62/6437212cf40e40b74dbc7e134700a21cb21a9ac7e46ade940b5d4826456f/black-24.2.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:1e08fb9a15c914b81dd734ddd7fb10513016e5ce7e6704bdd5e1251ceee51ac9", size = 1417360, upload-time = "2024-02-12T20:34:56.41Z" },
            { url = "https://files.pythonhosted.org/packages/36/8f/de0d339ae683422a8e15d6f74b8022d4947009c347d8c2178c303c68cc4d/black-24.2.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:810d445ae6069ce64030c78ff6127cd9cd178a9ac3361435708b907d8a04c693", size = 1739406, upload-time = "2024-02-12T20:23:59.596Z" },
            { url = "https://files.pythonhosted.org/packages/3e/58/89e5f5a1c4c5b66dc74eabe6337623d53b4d1c27fbbbe16defee53397f60/black-24.2.0-cp312-cp312-win_amd64.whl", hash = "sha256:ba15742a13de85e9b8f3239c8f807723991fbfae24bad92d34a2b12e81904982", size = 1373310, upload-time = "2024-02-12T20:25:27.243Z" },
            { url = "https://files.pythonhosted.org/packages/47/15/b3770bc3328685a53bc9c041136240146c5cd866a1f020c2cf47f2ff9683/black-24.2.0-py3-none-any.whl", hash = "sha256:e8a6ae970537e67830776488bca52000eaa37fa63b9988e8c487458d9cd5ace6", size = 200610, upload-time = "2024-02-12T20:21:17.657Z" },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121, upload-time = "2023-08-17T17:29:11.868Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941, upload-time = "2023-08-17T17:29:10.08Z" },
        ]

        [[package]]
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433, upload-time = "2023-02-04T12:11:27.157Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695, upload-time = "2023-02-04T12:11:25.002Z" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055, upload-time = "2024-01-31T01:00:36.02Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717, upload-time = "2024-01-31T01:00:34.019Z" },
        ]

        [tool]
        requirements = [{ name = "black", specifier = "==24.2.0" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-04-10T00:00:00Z"
        exclude-newer-span = "P3W"
        "#);
    });
}

#[test]
fn tool_install_python_from_global_version_file() {
    let context = uv_test::test_context_with_versions!(&["3.11", "3.12", "3.13"])
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Pin to 3.12
    context
        .python_pin()
        .arg("3.12")
        .arg("--global")
        .assert()
        .success();

    // Install a tool
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    Installed 1 executable: flask
    ");

    // It should use the version from the global file
    uv_snapshot!(context.filters(), Command::new("flask").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.2
    Werkzeug 3.0.1

    ----- stderr -----
    ");

    // Change global version
    context
        .python_pin()
        .arg("3.13")
        .arg("--global")
        .assert()
        .success();

    // Installing flask again should be a no-op, even though the global pin changed
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    `flask` is already installed
    ");

    uv_snapshot!(context.filters(), Command::new("flask").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.2
    Werkzeug 3.0.1

    ----- stderr -----
    ");

    // Using `--upgrade` forces us to check the environment
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .arg("--upgrade")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Checked [N] packages in [TIME]
    Installed 1 executable: flask
    ");

    // This will not change to the new global pin, since there was not a reinstall request
    uv_snapshot!(context.filters(), Command::new("flask").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.2
    Werkzeug 3.0.1

    ----- stderr -----
    ");

    // Using `--reinstall` forces us to install flask again
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .arg("--reinstall")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Ignoring existing environment for `flask`: the Python interpreter does not match the environment interpreter
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    Installed 1 executable: flask
    ");

    // This will change to the new global pin, since there was not an explicit request recorded in
    // the receipt
    uv_snapshot!(context.filters(), Command::new("flask").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.13.[X]
    Flask 3.0.2
    Werkzeug 3.0.1

    ----- stderr -----
    ");

    // If we request a specific Python version, it takes precedence over the pin
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .arg("--python")
        .arg("3.11")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Ignoring existing environment for `flask`: the requested Python interpreter does not match the environment interpreter
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    Installed 1 executable: flask
    ");

    uv_snapshot!(context.filters(), Command::new("flask").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.11.[X]
    Flask 3.0.2
    Werkzeug 3.0.1

    ----- stderr -----
    ");

    // Use `--reinstall` to install flask again
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .arg("--reinstall")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     ~ blinker==1.7.0
     ~ click==8.1.7
     ~ flask==3.0.2
     ~ itsdangerous==2.1.2
     ~ jinja2==3.1.3
     ~ markupsafe==2.1.5
     ~ werkzeug==3.0.1
    Installed 1 executable: flask
    ");

    // We should continue to use the version from the install, not the global pin
    uv_snapshot!(context.filters(), Command::new("flask").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.11.[X]
    Flask 3.0.2
    Werkzeug 3.0.1

    ----- stderr -----
    ");
}

#[test]
fn tool_install_force_respects_global_python_change() {
    let context = uv_test::test_context_with_versions!(&["3.11", "3.12", "3.13"])
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    context
        .python_pin()
        .arg("3.12")
        .arg("--global")
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    Installed 1 executable: flask
    ");

    uv_snapshot!(context.filters(), Command::new("flask").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.[X]
    Flask 3.0.2
    Werkzeug 3.0.1

    ----- stderr -----
    ");

    context
        .python_pin()
        .arg("3.13")
        .arg("--global")
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .arg("--force")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    Installed 1 executable: flask
    ");

    uv_snapshot!(context.filters(), Command::new("flask").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.13.[X]
    Flask 3.0.2
    Werkzeug 3.0.1

    ----- stderr -----
    ");
}

#[test]
fn tool_install_with_editable() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_exclude_newer("2025-01-18T00:00:00Z")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");
    let anyio_local = context.temp_dir.child("src").child("anyio_local");
    copy_dir_all(
        context.workspace_root.join("test/packages/anyio_local"),
        &anyio_local,
    )?;

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--with-editable")
        .arg("./src/anyio_local")
        .arg("--with")
        .arg("iniconfig")
        .arg("executable-application")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + anyio==4.3.0+foo (from file://[TEMP_DIR]/src/anyio_local)
     + executable-application==0.3.0
     + iniconfig==2.0.0
    Installed 1 executable: app
    ");

    Ok(())
}

#[test]
fn tool_install_workspace_members_do_not_override_explicit_with_requirements() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let with_editable_tool_dir = context.temp_dir.child("tools-with-editable");
    let with_editable_bin_dir = context.temp_dir.child("bin-with-editable");
    let with_tool_dir = context.temp_dir.child("tools-with");
    let with_bin_dir = context.temp_dir.child("bin-with");

    let root_pyproject = context.temp_dir.child("pyproject.toml");
    root_pyproject.write_str(indoc! {
        r#"
        [project]
        name = "root"
        version = "0.1.0"
        requires-python = ">=3.12"

        [project.scripts]
        root_cli = "root:main"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"

        [tool.uv.workspace]
        members = ["child"]
        "#
    })?;

    let root_src = context.temp_dir.child("src").child("root");
    root_src.create_dir_all()?;
    root_src.child("__init__.py").write_str(indoc! {
        r"
        def main():
            import child
            print(child.MESSAGE)
        "
    })?;

    let child = context.temp_dir.child("child");
    child.create_dir_all()?;
    child.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
    "#})?;

    let child_src = child.child("src").child("child");
    child_src.create_dir_all()?;
    child_src
        .child("__init__.py")
        .write_str("MESSAGE = 'OK'\n")?;

    let status = context
        .tool_install()
        .arg("--with-editable")
        .arg(child.path())
        .arg(context.temp_dir.path())
        .env(EnvVars::UV_TOOL_DIR, with_editable_tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, with_editable_bin_dir.as_os_str())
        .env(EnvVars::PATH, with_editable_bin_dir.as_os_str())
        .status()
        .expect("failed to run uv tool install with --with-editable");
    assert!(status.success());

    uv_snapshot!(context.filters(), Command::new("root_cli").env(EnvVars::PATH, with_editable_bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    OK

    ----- stderr -----
    ");

    child_src
        .child("__init__.py")
        .write_str("MESSAGE = 'CHANGED'\n")?;

    uv_snapshot!(context.filters(), Command::new("root_cli").env(EnvVars::PATH, with_editable_bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    CHANGED

    ----- stderr -----
    ");

    child_src
        .child("__init__.py")
        .write_str("MESSAGE = 'OK'\n")?;

    let status = context
        .tool_install()
        .arg("--editable")
        .arg("--with")
        .arg(child.path())
        .arg(context.temp_dir.path())
        .env(EnvVars::UV_TOOL_DIR, with_tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, with_bin_dir.as_os_str())
        .env(EnvVars::PATH, with_bin_dir.as_os_str())
        .status()
        .expect("failed to run uv tool install with --with");
    assert!(status.success());

    uv_snapshot!(context.filters(), Command::new("root_cli").env(EnvVars::PATH, with_bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    OK

    ----- stderr -----
    ");

    child_src
        .child("__init__.py")
        .write_str("MESSAGE = 'CHANGED'\n")?;

    uv_snapshot!(context.filters(), Command::new("root_cli").env(EnvVars::PATH, with_bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    OK

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn tool_install_preserves_mixed_workspace_member_editability() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let tool_root = context.temp_dir.child("tool-root");
    tool_root.create_dir_all()?;
    tool_root.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "tool-root"
        version = "0.1.0"
        requires-python = ">=3.12"

        [project.scripts]
        root_cli = "tool_root:main"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
    "#})?;
    let tool_root_src = tool_root.child("src").child("tool_root");
    tool_root_src.create_dir_all()?;
    tool_root_src.child("__init__.py").write_str(indoc! {
        r#"
        def main():
            import importlib.metadata
            import other_child

            print(f"{importlib.metadata.version('tool-root')} {other_child.MESSAGE}")
        "#
    })?;

    let other_workspace = context.temp_dir.child("other-workspace");
    other_workspace.create_dir_all()?;
    other_workspace
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "other-workspace"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["other-child"]

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"

        [tool.uv.sources]
        other-child = { workspace = true }

        [tool.uv.workspace]
        members = ["packages/*"]
    "#})?;
    let other_workspace_src = other_workspace.child("src").child("other_workspace");
    other_workspace_src.create_dir_all()?;
    other_workspace_src.child("__init__.py").touch()?;

    let other_child = other_workspace.child("packages").child("other-child");
    other_child.create_dir_all()?;
    other_child.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "other-child"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
    "#})?;
    let other_child_src = other_child.child("src").child("other_child");
    other_child_src.create_dir_all()?;
    other_child_src
        .child("__init__.py")
        .write_str("MESSAGE = 'OK'\n")?;

    let status = context
        .tool_install()
        .arg("--with-editable")
        .arg(other_workspace.path())
        .arg(tool_root.path())
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str())
        .status()
        .expect("failed to run uv tool install with mixed workspace editability");
    assert!(status.success());

    uv_snapshot!(context.filters(), Command::new("root_cli").env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    0.1.0 OK

    ----- stderr -----
    ");

    other_child_src
        .child("__init__.py")
        .write_str("MESSAGE = 'CHANGED'\n")?;

    uv_snapshot!(context.filters(), Command::new("root_cli").env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    0.1.0 CHANGED

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn tool_install_preserves_mixed_workspace_member_non_editability() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let tool_root = context.temp_dir.child("tool-root");
    tool_root.create_dir_all()?;
    tool_root.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "tool-root"
        version = "0.1.0"
        requires-python = ">=3.12"

        [project.scripts]
        root_cli = "tool_root:main"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
    "#})?;
    let tool_root_src = tool_root.child("src").child("tool_root");
    tool_root_src.create_dir_all()?;
    tool_root_src.child("__init__.py").write_str(indoc! {
        r#"
        def main():
            import importlib.metadata
            import other_child

            print(f"{importlib.metadata.version('tool-root')} {other_child.MESSAGE}")
        "#
    })?;

    let other_workspace = context.temp_dir.child("other-workspace");
    other_workspace.create_dir_all()?;
    other_workspace
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "other-workspace"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["other-child"]

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"

        [tool.uv.sources]
        other-child = { workspace = true }

        [tool.uv.workspace]
        members = ["packages/*"]
    "#})?;
    let other_workspace_src = other_workspace.child("src").child("other_workspace");
    other_workspace_src.create_dir_all()?;
    other_workspace_src.child("__init__.py").touch()?;

    let other_child = other_workspace.child("packages").child("other-child");
    other_child.create_dir_all()?;
    other_child.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "other-child"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
    "#})?;
    let other_child_src = other_child.child("src").child("other_child");
    other_child_src.create_dir_all()?;
    other_child_src
        .child("__init__.py")
        .write_str("MESSAGE = 'OK'\n")?;

    let status = context
        .tool_install()
        .arg("--editable")
        .arg("--with")
        .arg(other_workspace.path())
        .arg(tool_root.path())
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str())
        .status()
        .expect("failed to run uv tool install with mixed workspace editability");
    assert!(status.success());

    uv_snapshot!(context.filters(), Command::new("root_cli").env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    0.1.0 OK

    ----- stderr -----
    ");

    other_child_src
        .child("__init__.py")
        .write_str("MESSAGE = 'CHANGED'\n")?;

    uv_snapshot!(context.filters(), Command::new("root_cli").env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    0.1.0 OK

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn tool_install_reinstall_converts_workspace_members_to_non_editable() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let root_pyproject = context.temp_dir.child("pyproject.toml");
    root_pyproject.write_str(indoc! {
        r#"
        [project]
        name = "root"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["child"]

        [project.scripts]
        root_cli = "root:main"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"

        [tool.uv.sources]
        child = { workspace = true }

        [tool.uv.workspace]
        members = ["child"]
        "#
    })?;

    let root_src = context.temp_dir.child("src").child("root");
    root_src.create_dir_all()?;
    root_src.child("__init__.py").write_str(indoc! {
        r"
        def main():
            import child
            print(child.MESSAGE)
        "
    })?;

    let child = context.temp_dir.child("child");
    child.create_dir_all()?;
    child.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
    "#})?;

    let child_src = child.child("src").child("child");
    child_src.create_dir_all()?;
    child_src
        .child("__init__.py")
        .write_str("MESSAGE = 'OK'\n")?;

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--editable")
        .arg(context.temp_dir.path())
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
     + root==0.1.0 (from file://[TEMP_DIR]/)
    Installed 1 executable: root_cli
    ");

    uv_snapshot!(context.filters(), Command::new("root_cli").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    OK

    ----- stderr -----
    ");

    let status = context
        .tool_install()
        .arg("--reinstall")
        .arg(context.temp_dir.path())
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str())
        .status()
        .expect("failed to run uv tool install --reinstall");
    assert!(status.success());

    child_src
        .child("__init__.py")
        .write_str("MESSAGE = 'CHANGED'\n")?;

    uv_snapshot!(context.filters(), Command::new("root_cli").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    OK

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn tool_install_workspace_members_are_non_editable_by_default() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let root_pyproject = context.temp_dir.child("pyproject.toml");
    root_pyproject.write_str(indoc! {
        r#"
        [project]
        name = "root"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["child"]

        [project.scripts]
        root_cli = "root:main"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"

        [tool.uv.sources]
        child = { workspace = true }

        [tool.uv.workspace]
        members = ["child"]
        "#
    })?;

    let root_src = context.temp_dir.child("src").child("root");
    root_src.create_dir_all()?;
    root_src.child("__init__.py").write_str(indoc! {
        r"
        def main():
            import child
            print(child.MESSAGE)
        "
    })?;

    let child = context.temp_dir.child("child");
    child.create_dir_all()?;
    child.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
    "#})?;

    let child_src = child.child("src").child("child");
    child_src.create_dir_all()?;
    child_src
        .child("__init__.py")
        .write_str("MESSAGE = 'OK'\n")?;

    uv_snapshot!(context.filters(), context.tool_install()
        .arg(context.temp_dir.path())
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
     + root==0.1.0 (from file://[TEMP_DIR]/)
    Installed 1 executable: root_cli
    ");

    uv_snapshot!(context.filters(), Command::new("root_cli").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    OK

    ----- stderr -----
    ");

    child_src
        .child("__init__.py")
        .write_str("MESSAGE = 'CHANGED'\n")?;

    uv_snapshot!(context.filters(), Command::new("root_cli").env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    OK

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn tool_install_repairs_workspace_member_editability_drift_on_reinstall_check() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let root_pyproject = context.temp_dir.child("pyproject.toml");
    root_pyproject.write_str(indoc! {
        r#"
        [project]
        name = "root"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["child"]

        [project.scripts]
        root_cli = "root:main"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"

        [tool.uv.sources]
        child = { workspace = true }

        [tool.uv.workspace]
        members = ["child"]
        "#
    })?;

    let root_src = context.temp_dir.child("src").child("root");
    root_src.create_dir_all()?;
    root_src.child("__init__.py").write_str(indoc! {
        r"
        def main():
            import child
            print(child.MESSAGE)
        "
    })?;

    let child = context.temp_dir.child("child");
    child.create_dir_all()?;
    child.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
    "#})?;

    let child_src = child.child("src").child("child");
    child_src.create_dir_all()?;
    child_src
        .child("__init__.py")
        .write_str("MESSAGE = 'OK'\n")?;

    uv_snapshot!(context.filters(), context.tool_install()
        .arg(context.temp_dir.path())
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
     + root==0.1.0 (from file://[TEMP_DIR]/)
    Installed 1 executable: root_cli
    ");

    let tool_python = if cfg!(windows) {
        tool_dir
            .join("root")
            .join("Scripts")
            .join(format!("python{}", std::env::consts::EXE_SUFFIX))
    } else {
        tool_dir.join("root").join("bin").join("python")
    };
    let status = context
        .pip_install()
        .arg("--python")
        .arg(tool_python.as_os_str())
        .arg("--editable")
        .arg(child.path())
        .status()
        .expect("failed to drift workspace member to editable");
    assert!(status.success());

    child_src
        .child("__init__.py")
        .write_str("MESSAGE = 'DRIFTED'\n")?;

    uv_snapshot!(context.filters(), Command::new("root_cli").env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    DRIFTED

    ----- stderr -----
    ");

    uv_snapshot!(context.filters(), context.tool_install()
        .arg(context.temp_dir.path())
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ child==0.1.0 (from file://[TEMP_DIR]/child)
    Installed 1 executable: root_cli
    ");

    child_src
        .child("__init__.py")
        .write_str("MESSAGE = 'POST-REPAIR'\n")?;

    uv_snapshot!(context.filters(), Command::new("root_cli").env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    OK

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn tool_install_workspace_members_honor_editable_flag() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let root_pyproject = context.temp_dir.child("pyproject.toml");
    root_pyproject.write_str(indoc! {
        r#"
        [project]
        name = "root"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["child"]

        [project.scripts]
        root_cli = "root:main"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"

        [tool.uv.sources]
        child = { workspace = true }

        [tool.uv.workspace]
        members = ["child"]
        "#
    })?;

    let root_src = context.temp_dir.child("src").child("root");
    root_src.create_dir_all()?;
    root_src.child("__init__.py").write_str(indoc! {
        r"
        def main():
            import child
            print(child.MESSAGE)
        "
    })?;

    let child = context.temp_dir.child("child");
    child.create_dir_all()?;
    child.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
    "#})?;

    let child_src = child.child("src").child("child");
    child_src.create_dir_all()?;
    child_src
        .child("__init__.py")
        .write_str("MESSAGE = 'OK'\n")?;

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--editable")
        .arg(context.temp_dir.path())
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
     + root==0.1.0 (from file://[TEMP_DIR]/)
    Installed 1 executable: root_cli
    ");

    uv_snapshot!(context.filters(), Command::new("root_cli").env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    OK

    ----- stderr -----
    ");

    child_src
        .child("__init__.py")
        .write_str("MESSAGE = 'CHANGED'\n")?;

    uv_snapshot!(context.filters(), Command::new("root_cli").env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    CHANGED

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn tool_install_workspace_members_honor_source_editable_flag() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let root_pyproject = context.temp_dir.child("pyproject.toml");
    root_pyproject.write_str(indoc! {
        r#"
        [project]
        name = "root"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["child"]

        [project.scripts]
        root_cli = "root:main"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"

        [tool.uv.sources]
        child = { workspace = true, editable = true }

        [tool.uv.workspace]
        members = ["child"]
        "#
    })?;

    let root_src = context.temp_dir.child("src").child("root");
    root_src.create_dir_all()?;
    root_src.child("__init__.py").write_str(indoc! {
        r"
        ROOT_MESSAGE = 'ROOT'

        def main():
            import child
            print(f'{ROOT_MESSAGE} {child.MESSAGE}')
        "
    })?;

    let child = context.temp_dir.child("child");
    child.create_dir_all()?;
    child.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "child"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["uv_build>=0.7,<10000"]
        build-backend = "uv_build"
    "#})?;

    let child_src = child.child("src").child("child");
    child_src.create_dir_all()?;
    child_src
        .child("__init__.py")
        .write_str("MESSAGE = 'OK'\n")?;

    uv_snapshot!(context.filters(), context.tool_install()
        .arg(context.temp_dir.path())
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + child==0.1.0 (from file://[TEMP_DIR]/child)
     + root==0.1.0 (from file://[TEMP_DIR]/)
    Installed 1 executable: root_cli
    ");

    uv_snapshot!(context.filters(), Command::new("root_cli").env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    ROOT OK

    ----- stderr -----
    ");

    root_src.child("__init__.py").write_str(indoc! {
        r"
        ROOT_MESSAGE = 'CHANGED'

        def main():
            import child
            print(f'{ROOT_MESSAGE} {child.MESSAGE}')
        "
    })?;

    uv_snapshot!(context.filters(), Command::new("root_cli").env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    ROOT OK

    ----- stderr -----
    ");

    child_src
        .child("__init__.py")
        .write_str("MESSAGE = 'CHANGED'\n")?;

    uv_snapshot!(context.filters(), Command::new("root_cli").env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    ROOT CHANGED

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn tool_install_with_compatible_build_constraints() -> Result<()> {
    let context = uv_test::test_context!("3.9")
        .with_exclude_newer("2024-05-04T00:00:00Z")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let constraints_txt = context.temp_dir.child("build_constraints.txt");
    constraints_txt.write_str("setuptools>=40")?;

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("black")
        .arg("--with")
        .arg("requests==1.2")
        .arg("--build-constraints")
        .arg("build_constraints.txt")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.4.2
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.1
     + requests==1.2.0
     + tomli==2.0.1
     + typing-extensions==4.11.0
    Installed 2 executables: black, blackd
    ");

    tool_dir
        .child("black")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.9.[X]"

        [options]
        exclude-newer = "2024-05-04T00:00:00Z"

        [manifest]
        requirements = [
            { name = "black" },
            { name = "requests", specifier = "==1.2" },
        ]
        build-constraints = [{ name = "setuptools", specifier = ">=40" }]

        [[package]]
        name = "black"
        version = "24.4.2"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "click" },
            { name = "mypy-extensions" },
            { name = "packaging" },
            { name = "pathspec" },
            { name = "platformdirs" },
            { name = "tomli", marker = "python_full_version < '3.11'" },
            { name = "typing-extensions", marker = "python_full_version < '3.11'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/a2/47/c9997eb470a7f48f7aaddd3d9a828244a2e4199569e38128715c48059ac1/black-24.4.2.tar.gz", hash = "sha256:c872b53057f000085da66a19c55d68f6f8ddcac2642392ad3a355878406fbd4d", size = 642299, upload-time = "2024-04-26T00:32:15.305Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/40/f6/3adc48c210527a7b651aaed43824a9b8bd04b3fb361a5227bad046e1c876/black-24.4.2-cp310-cp310-macosx_10_9_x86_64.whl", hash = "sha256:dd1b5a14e417189db4c7b64a6540f31730713d173f0b63e55fabd52d61d8fdce", size = 1631487, upload-time = "2024-04-26T00:40:28.969Z" },
            { url = "https://files.pythonhosted.org/packages/a2/25/70aa1bec12c841a03e333e312daa0cf2fee50ea6336ac4851c93c0e2b411/black-24.4.2-cp310-cp310-macosx_11_0_arm64.whl", hash = "sha256:8e537d281831ad0e71007dcdcbe50a71470b978c453fa41ce77186bbe0ed6021", size = 1456317, upload-time = "2024-04-26T00:39:10.333Z" },
            { url = "https://files.pythonhosted.org/packages/e0/7d/7f8df0fdbbbefc4362d3eca6b69b7a8a4249a8a88dabc00a207d31fddcd7/black-24.4.2-cp310-cp310-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:eaea3008c281f1038edb473c1aa8ed8143a5535ff18f978a318f10302b254063", size = 1822765, upload-time = "2024-04-26T00:34:56.436Z" },
            { url = "https://files.pythonhosted.org/packages/5c/21/1ee97841c469c1551133cbe47448cdba9628c7d9431f74f114f02e3b233c/black-24.4.2-cp310-cp310-win_amd64.whl", hash = "sha256:7768a0dbf16a39aa5e9a3ded568bb545c8c2727396d063bbaf847df05b08cd96", size = 1409336, upload-time = "2024-04-26T00:35:30.392Z" },
            { url = "https://files.pythonhosted.org/packages/9b/f7/591d601c3046ceb65b97291dfe87fa25124cffac3d97aaaba89d0f0d7bdf/black-24.4.2-cp311-cp311-macosx_10_9_x86_64.whl", hash = "sha256:257d724c2c9b1660f353b36c802ccece186a30accc7742c176d29c146df6e474", size = 1615013, upload-time = "2024-04-26T00:39:49.415Z" },
            { url = "https://files.pythonhosted.org/packages/c9/17/5e0036b265bbf6bc44970d93d48febcbc03701b671db3c9603fd43ebc616/black-24.4.2-cp311-cp311-macosx_11_0_arm64.whl", hash = "sha256:bdde6f877a18f24844e381d45e9947a49e97933573ac9d4345399be37621e26c", size = 1436163, upload-time = "2024-04-26T00:40:20.267Z" },
            { url = "https://files.pythonhosted.org/packages/c5/48/34176b522e8cff4620a5d96c2e323ff2413f574870eb25efa8025885e028/black-24.4.2-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:e151054aa00bad1f4e1f04919542885f89f5f7d086b8a59e5000e6c616896ffb", size = 1803382, upload-time = "2024-04-26T00:34:38.665Z" },
            { url = "https://files.pythonhosted.org/packages/74/ce/e8eec1a77edbfa982bee3b5460dcdd4fe0e4e3165fc15d8ec44d04da7776/black-24.4.2-cp311-cp311-win_amd64.whl", hash = "sha256:7e122b1c4fb252fd85df3ca93578732b4749d9be076593076ef4d07a0233c3e1", size = 1417802, upload-time = "2024-04-26T00:35:08.804Z" },
            { url = "https://files.pythonhosted.org/packages/f4/75/3a29de3bda4006cc280d833b5d961cf7df3810a21f49e7a63a7e551fb351/black-24.4.2-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:accf49e151c8ed2c0cdc528691838afd217c50412534e876a19270fea1e28e2d", size = 1645176, upload-time = "2024-04-26T00:42:35.606Z" },
            { url = "https://files.pythonhosted.org/packages/be/b8/9c152301774fa62a265b035a8ede4d6280827904ea1af8c3be10a28d3187/black-24.4.2-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:88c57dc656038f1ab9f92b3eb5335ee9b021412feaa46330d5eba4e51fe49b04", size = 1446227, upload-time = "2024-04-26T00:40:35.195Z" },
            { url = "https://files.pythonhosted.org/packages/25/6d/eb15a1b155f755f43766cc473618c6e1de6555d6a1764965643f486dcf01/black-24.4.2-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:be8bef99eb46d5021bf053114442914baeb3649a89dc5f3a555c88737e5e98fc", size = 1832011, upload-time = "2024-04-26T00:34:37.825Z" },
            { url = "https://files.pythonhosted.org/packages/43/24/942b22571b0171be7c6f701cdc3e3b7221f5b522ef02cf82503a547a657b/black-24.4.2-cp312-cp312-win_amd64.whl", hash = "sha256:415e686e87dbbe6f4cd5ef0fbf764af7b89f9057b97c908742b6008cc554b9c0", size = 1428800, upload-time = "2024-04-26T00:35:55.838Z" },
            { url = "https://files.pythonhosted.org/packages/e9/1e/10035a567cb756a74d63330389c19e9b60acc958f7cb243cd01a313fae28/black-24.4.2-cp39-cp39-macosx_10_9_x86_64.whl", hash = "sha256:37aae07b029fa0174d39daf02748b379399b909652a806e5708199bd93899da1", size = 1630974, upload-time = "2024-04-26T00:43:01.788Z" },
            { url = "https://files.pythonhosted.org/packages/21/88/56320cb37945dcdc903a3024d09f2b102c69bdd586c6b73ea138e679615a/black-24.4.2-cp39-cp39-macosx_11_0_arm64.whl", hash = "sha256:da33a1a5e49c4122ccdfd56cd021ff1ebc4a1ec4e2d01594fef9b6f267a9e741", size = 1455391, upload-time = "2024-04-26T00:42:01.383Z" },
            { url = "https://files.pythonhosted.org/packages/d0/f9/9d1bda779af428e40b2e561b6509f3eb56f3190e3865663e1234e2a657c7/black-24.4.2-cp39-cp39-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:ef703f83fc32e131e9bcc0a5094cfe85599e7109f896fe8bc96cc402f3eb4b6e", size = 1820933, upload-time = "2024-04-26T00:34:55.649Z" },
            { url = "https://files.pythonhosted.org/packages/57/b2/05a06a65c17c5b092fd5a22de912dd0bfb5bb20646fb66821607263b6096/black-24.4.2-cp39-cp39-win_amd64.whl", hash = "sha256:b9176b9832e84308818a99a561e90aa479e73c523b3f77afd07913380ae2eab7", size = 1408503, upload-time = "2024-04-26T00:37:02.792Z" },
            { url = "https://files.pythonhosted.org/packages/0f/89/294c9a6b6c75a08da55e9d05321d0707e9418735e3062b12ef0f54c33474/black-24.4.2-py3-none-any.whl", hash = "sha256:d36ed1124bb81b32f8614555b34cc4259c3fbc7eec17870e8ff8ded335b58d8c", size = 205925, upload-time = "2024-04-26T00:32:12.495Z" },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121, upload-time = "2023-08-17T17:29:11.868Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941, upload-time = "2023-08-17T17:29:10.08Z" },
        ]

        [[package]]
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433, upload-time = "2023-02-04T12:11:27.157Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695, upload-time = "2023-02-04T12:11:25.002Z" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/b2/e4/2856bf61e54d7e3a03dd00d0c1b5fa86e6081e8f262eb91befbe64d20937/platformdirs-4.2.1.tar.gz", hash = "sha256:031cd18d4ec63ec53e82dceaac0417d218a6863f7745dfcc9efe7793b7039bdf", size = 20398, upload-time = "2024-04-23T16:47:28.19Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b0/15/1691fa5aaddc0c4ea4901c26f6137c29d5f6673596fe960a0340e8c308e1/platformdirs-4.2.1-py3-none-any.whl", hash = "sha256:17d5a1161b3fd67b390023cb2d3b026bbd40abde6fdb052dfbd3a29c3ba22ee1", size = 17770, upload-time = "2024-04-23T16:47:26.512Z" },
        ]

        [[package]]
        name = "requests"
        version = "1.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/37/e4/74cb55b3da7777a1dc7cd7985c3cb12e83e213c03b0f9ca20d2c0e92b3c3/requests-1.2.0.tar.gz", hash = "sha256:cfa615644ae38efe8423ce9edb23470a4615a9147fa3cea5026afb47c9bb3913", size = 341511, upload-time = "2013-03-31T05:28:47.574Z" }

        [[package]]
        name = "tomli"
        version = "2.0.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/c0/3f/d7af728f075fb08564c5949a9c95e44352e23dee646869fa104a3b2060a3/tomli-2.0.1.tar.gz", hash = "sha256:de526c12914f0c550d15924c62d72abc48d6fe7364aa87328337a31007fe8a4f", size = 15164, upload-time = "2022-02-08T10:54:04.006Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/97/75/10a9ebee3fd790d20926a90a2547f0bf78f371b2f13aa822c759680ca7b9/tomli-2.0.1-py3-none-any.whl", hash = "sha256:939de3e7a6161af0c887ef91b7d41a53e7c5a1ca976325f429cb46ea9bc30ecc", size = 12757, upload-time = "2022-02-08T10:54:02.017Z" },
        ]

        [[package]]
        name = "typing-extensions"
        version = "4.11.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/f6/f3/b827b3ab53b4e3d8513914586dcca61c355fa2ce8252dea4da56e67bf8f2/typing_extensions-4.11.0.tar.gz", hash = "sha256:83f085bd5ca59c80295fc2a82ab5dac679cbe02b9f33f7d83af68e241bea51b0", size = 78744, upload-time = "2024-04-05T12:35:47.093Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/01/f3/936e209267d6ef7510322191003885de524fc48d1b43269810cd589ceaf5/typing_extensions-4.11.0-py3-none-any.whl", hash = "sha256:c1f94d72897edaf4ce775bb7558d5b79d8126906a14ea5ed1635921406c0387a", size = 34698, upload-time = "2024-04-05T12:35:44.388Z" },
        ]

        [tool]
        requirements = [
            { name = "black" },
            { name = "requests", specifier = "==1.2" },
        ]
        build-constraint-dependencies = [{ name = "setuptools", specifier = ">=40" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-05-04T00:00:00Z"
        "#);
    });

    Ok(())
}

#[test]
fn tool_install_with_incompatible_build_constraints() -> Result<()> {
    let context = uv_test::test_context!("3.9")
        .with_exclude_newer("2024-05-04T00:00:00Z")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let constraints_txt = context.temp_dir.child("build_constraints.txt");
    constraints_txt.write_str("setuptools==2")?;

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--with")
        .arg("requests==1.2")
        .arg("--build-constraints")
        .arg("build_constraints.txt")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × Failed to download and build `requests==1.2.0`
      ├─▶ Failed to resolve requirements from `setup.py` build
      ├─▶ No solution found when resolving: `setuptools>=40.8.0`
      ╰─▶ Because you require setuptools>=40.8.0 and setuptools==2, we can conclude that your requirements are unsatisfiable.
    ");

    tool_dir
        .child("black")
        .child("uv-receipt.toml")
        .assert(predicate::path::missing());

    Ok(())
}

#[test]
fn tool_install_suggest_other_packages_with_executable() {
    // FastAPI 0.111 is only available from this date onwards.
    let context = uv_test::test_context!("3.12")
        .with_exclude_newer("2024-05-04T00:00:00Z")
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");
    let context = context.with_filter(("\\+ uvloop(.+)\n ", ""));

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("fastapi==0.111.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----
    No executables are provided by package `fastapi`; removing tool
    hint: An executable with the name `fastapi` is available via dependency `fastapi-cli`.
          Did you mean `uv tool install fastapi-cli`?

    ----- stderr -----
    Resolved 35 packages in [TIME]
    Prepared 35 packages in [TIME]
    Installed 35 packages in [TIME]
     + annotated-types==0.6.0
     + anyio==4.3.0
     + certifi==2024.2.2
     + click==8.1.7
     + dnspython==2.6.1
     + email-validator==2.1.1
     + fastapi==0.111.0
     + fastapi-cli==0.0.2
     + h11==0.14.0
     + httpcore==1.0.5
     + httptools==0.6.1
     + httpx==0.27.0
     + idna==3.7
     + jinja2==3.1.3
     + markdown-it-py==3.0.0
     + markupsafe==2.1.5
     + mdurl==0.1.2
     + orjson==3.10.3
     + pydantic==2.7.1
     + pydantic-core==2.18.2
     + pygments==2.17.2
     + python-dotenv==1.0.1
     + python-multipart==0.0.9
     + pyyaml==6.0.1
     + rich==13.7.1
     + shellingham==1.5.4
     + sniffio==1.3.1
     + starlette==0.37.2
     + typer==0.12.3
     + typing-extensions==4.11.0
     + ujson==5.9.0
     + uvicorn==0.29.0
     + watchfiles==0.21.0
     + websockets==12.0
    error: Failed to install entrypoints for `fastapi`
    ");
}

/// Test installing a tool at a version
#[test]
fn tool_install_version() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_exe_suffix()
        .with_filtered_compiled();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.2.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    tool_dir.child("black").assert(predicate::path::is_dir());
    tool_dir
        .child("black")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("black{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run black in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r#"
        #![TEMP_DIR]/tools/black/bin/python
        # -*- coding: utf-8 -*-
        import sys
        from black import patched_main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(patched_main())
        "#);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "black", specifier = "==24.2.0" }]

        [[package]]
        name = "black"
        version = "24.2.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "click" },
            { name = "mypy-extensions" },
            { name = "packaging" },
            { name = "pathspec" },
            { name = "platformdirs" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/29/69/f3ab49cdb938b3eecb048fa64f86bdadb1fac26e92c435d287181d543b0a/black-24.2.0.tar.gz", hash = "sha256:bce4f25c27c3435e4dace4815bcb2008b87e167e3bf4ee47ccdc5ce906eb4894", size = 631598, upload-time = "2024-02-12T20:21:26.969Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/43/1e/67c87a1fb39592aa944f35cc26892946ebe0a10aa324b87f9380b8753862/black-24.2.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:d84f29eb3ee44859052073b7636533ec995bd0f64e2fb43aeceefc70090e752b", size = 1585288, upload-time = "2024-02-12T20:37:13.8Z" },
            { url = "https://files.pythonhosted.org/packages/5e/62/6437212cf40e40b74dbc7e134700a21cb21a9ac7e46ade940b5d4826456f/black-24.2.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:1e08fb9a15c914b81dd734ddd7fb10513016e5ce7e6704bdd5e1251ceee51ac9", size = 1417360, upload-time = "2024-02-12T20:34:56.41Z" },
            { url = "https://files.pythonhosted.org/packages/36/8f/de0d339ae683422a8e15d6f74b8022d4947009c347d8c2178c303c68cc4d/black-24.2.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:810d445ae6069ce64030c78ff6127cd9cd178a9ac3361435708b907d8a04c693", size = 1739406, upload-time = "2024-02-12T20:23:59.596Z" },
            { url = "https://files.pythonhosted.org/packages/3e/58/89e5f5a1c4c5b66dc74eabe6337623d53b4d1c27fbbbe16defee53397f60/black-24.2.0-cp312-cp312-win_amd64.whl", hash = "sha256:ba15742a13de85e9b8f3239c8f807723991fbfae24bad92d34a2b12e81904982", size = 1373310, upload-time = "2024-02-12T20:25:27.243Z" },
            { url = "https://files.pythonhosted.org/packages/47/15/b3770bc3328685a53bc9c041136240146c5cd866a1f020c2cf47f2ff9683/black-24.2.0-py3-none-any.whl", hash = "sha256:e8a6ae970537e67830776488bca52000eaa37fa63b9988e8c487458d9cd5ace6", size = 200610, upload-time = "2024-02-12T20:21:17.657Z" },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121, upload-time = "2023-08-17T17:29:11.868Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941, upload-time = "2023-08-17T17:29:10.08Z" },
        ]

        [[package]]
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433, upload-time = "2023-02-04T12:11:27.157Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695, upload-time = "2023-02-04T12:11:25.002Z" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055, upload-time = "2024-01-31T01:00:36.02Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717, upload-time = "2024-01-31T01:00:34.019Z" },
        ]

        [tool]
        requirements = [{ name = "black", specifier = "==24.2.0" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    uv_snapshot!(context.filters(), Command::new("black").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.2.0 (compiled: [COMPILED])
    Python (CPython) 3.12.[X]

    ----- stderr -----
    ");
}

/// Test an editable installation of a tool.
#[test]
fn tool_install_editable() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` as an editable package.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("-e")
        .arg(context.workspace_root.join("test/packages/black_editable"))
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + black==0.1.0 (from file://[WORKSPACE]/test/packages/black_editable)
    Installed 1 executable: black
    ");

    tool_dir.child("black").assert(predicate::path::is_dir());
    tool_dir
        .child("black")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("black{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run black in the virtual environment
        assert_snapshot!(fs_err::read_to_string(&executable).unwrap(), @r#"
        #![TEMP_DIR]/tools/black/bin/python
        # -*- coding: utf-8 -*-
        import sys
        from black import main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(main())
        "#);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "black", editable = "[WORKSPACE]/test/packages/black_editable" }]

        [[package]]
        name = "black"
        version = "0.1.0"
        source = { editable = "[WORKSPACE]/test/packages/black_editable" }

        [package.metadata]
        requires-dist = [
            { name = "aiohttp", marker = "implementation_name == 'pypy' and sys_platform == 'win32' and extra == 'd'", specifier = ">=3.7.4,!=3.9.0" },
            { name = "aiohttp", marker = "(implementation_name != 'pypy' and extra == 'd') or (sys_platform != 'win32' and extra == 'd')", specifier = ">=3.7.4" },
            { name = "black", extras = ["d"], marker = "extra == 'dev'" },
            { name = "black", extras = ["uvloop"], marker = "extra == 'dev'" },
            { name = "colorama", marker = "extra == 'colorama'", specifier = ">=0.4.3" },
            { name = "ipython", marker = "extra == 'jupyter'", specifier = ">=7.8.0" },
            { name = "tokenize-rt", marker = "extra == 'jupyter'", specifier = ">=3.2.0" },
            { name = "uvloop", marker = "extra == 'uvloop'", specifier = ">=0.15.2" },
        ]
        provides-extras = ["colorama", "uvloop", "d", "jupyter", "dev"]

        [tool]
        requirements = [{ name = "black", editable = "[WORKSPACE]/test/packages/black_editable" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    uv_snapshot!(context.filters(), Command::new("black").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello world!

    ----- stderr -----
    ");

    // Request `black`. It should reinstall from the registry.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Checked 1 package in [TIME]
    Installed 1 executable: black
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "black" }]

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
        sdist = { url = "https://files.pythonhosted.org/packages/8f/5f/bac24a952668c7482cfdb4ebf91ba57a796c9da8829363a772040c1a3312/black-24.3.0.tar.gz", hash = "sha256:a0c9c4a0771afc6919578cec71ce82a3e31e054904e7197deacbc9382671c41f", size = 634292, upload-time = "2024-03-15T19:35:43.699Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b6/c6/1d174efa9ff02b22d0124c73fc5f4d4fb006d0d9a081aadc354d05754a13/black-24.3.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:2818cf72dfd5d289e48f37ccfa08b460bf469e67fb7c4abb07edc2e9f16fb63f", size = 1600822, upload-time = "2024-03-15T19:45:20.337Z" },
            { url = "https://files.pythonhosted.org/packages/d9/ed/704731afffe460b8ff0672623b40fce9fe569f2ee617c15857e4d4440a3a/black-24.3.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:4acf672def7eb1725f41f38bf6bf425c8237248bb0804faa3965c036f7672d11", size = 1429987, upload-time = "2024-03-15T19:45:00.637Z" },
            { url = "https://files.pythonhosted.org/packages/a8/05/8dd038e30caadab7120176d4bc109b7ca2f4457f12eef746b0560a583458/black-24.3.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:c7ed6668cbbfcd231fa0dc1b137d3e40c04c7f786e626b405c62bcd5db5857e4", size = 1755319, upload-time = "2024-03-15T19:38:24.009Z" },
            { url = "https://files.pythonhosted.org/packages/71/9d/e5fa1ff4ef1940be15a64883c0bb8d2fcf626efec996eab4ae5a8c691d2c/black-24.3.0-cp312-cp312-win_amd64.whl", hash = "sha256:56f52cfbd3dabe2798d76dbdd299faa046a901041faf2cf33288bc4e6dae57b5", size = 1385180, upload-time = "2024-03-15T19:39:37.014Z" },
            { url = "https://files.pythonhosted.org/packages/4d/ea/31770a7e49f3eedfd8cd7b35e78b3a3aaad860400f8673994bc988318135/black-24.3.0-py3-none-any.whl", hash = "sha256:41622020d7120e01d377f74249e677039d20e6344ff5851de8a10f11f513bf93", size = 201493, upload-time = "2024-03-15T19:35:41.572Z" },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121, upload-time = "2023-08-17T17:29:11.868Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941, upload-time = "2023-08-17T17:29:10.08Z" },
        ]

        [[package]]
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433, upload-time = "2023-02-04T12:11:27.157Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695, upload-time = "2023-02-04T12:11:25.002Z" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055, upload-time = "2024-01-31T01:00:36.02Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717, upload-time = "2024-01-31T01:00:34.019Z" },
        ]

        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Request `black` at a different version. It should install a new version.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("black")
        .arg("--from")
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 6 packages in [TIME]
     - black==0.1.0 (from file://[WORKSPACE]/test/packages/black_editable)
     + black==24.2.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "black", specifier = "==24.2.0" }]

        [[package]]
        name = "black"
        version = "24.2.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "click" },
            { name = "mypy-extensions" },
            { name = "packaging" },
            { name = "pathspec" },
            { name = "platformdirs" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/29/69/f3ab49cdb938b3eecb048fa64f86bdadb1fac26e92c435d287181d543b0a/black-24.2.0.tar.gz", hash = "sha256:bce4f25c27c3435e4dace4815bcb2008b87e167e3bf4ee47ccdc5ce906eb4894", size = 631598, upload-time = "2024-02-12T20:21:26.969Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/43/1e/67c87a1fb39592aa944f35cc26892946ebe0a10aa324b87f9380b8753862/black-24.2.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:d84f29eb3ee44859052073b7636533ec995bd0f64e2fb43aeceefc70090e752b", size = 1585288, upload-time = "2024-02-12T20:37:13.8Z" },
            { url = "https://files.pythonhosted.org/packages/5e/62/6437212cf40e40b74dbc7e134700a21cb21a9ac7e46ade940b5d4826456f/black-24.2.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:1e08fb9a15c914b81dd734ddd7fb10513016e5ce7e6704bdd5e1251ceee51ac9", size = 1417360, upload-time = "2024-02-12T20:34:56.41Z" },
            { url = "https://files.pythonhosted.org/packages/36/8f/de0d339ae683422a8e15d6f74b8022d4947009c347d8c2178c303c68cc4d/black-24.2.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:810d445ae6069ce64030c78ff6127cd9cd178a9ac3361435708b907d8a04c693", size = 1739406, upload-time = "2024-02-12T20:23:59.596Z" },
            { url = "https://files.pythonhosted.org/packages/3e/58/89e5f5a1c4c5b66dc74eabe6337623d53b4d1c27fbbbe16defee53397f60/black-24.2.0-cp312-cp312-win_amd64.whl", hash = "sha256:ba15742a13de85e9b8f3239c8f807723991fbfae24bad92d34a2b12e81904982", size = 1373310, upload-time = "2024-02-12T20:25:27.243Z" },
            { url = "https://files.pythonhosted.org/packages/47/15/b3770bc3328685a53bc9c041136240146c5cd866a1f020c2cf47f2ff9683/black-24.2.0-py3-none-any.whl", hash = "sha256:e8a6ae970537e67830776488bca52000eaa37fa63b9988e8c487458d9cd5ace6", size = 200610, upload-time = "2024-02-12T20:21:17.657Z" },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121, upload-time = "2023-08-17T17:29:11.868Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941, upload-time = "2023-08-17T17:29:10.08Z" },
        ]

        [[package]]
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433, upload-time = "2023-02-04T12:11:27.157Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695, upload-time = "2023-02-04T12:11:25.002Z" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055, upload-time = "2024-01-31T01:00:36.02Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717, upload-time = "2024-01-31T01:00:34.019Z" },
        ]

        [tool]
        requirements = [{ name = "black", specifier = "==24.2.0" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });
}

/// Ensure that we remove any existing entrypoints upon error.
#[test]
fn tool_install_remove_on_empty() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `pyflakes`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("pyflakes")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + pyflakes==3.2.0
    Installed 1 executable: pyflakes
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("pyflakes").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "pyflakes" }]

        [[package]]
        name = "pyflakes"
        version = "3.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/57/f9/669d8c9c86613c9d568757c7f5824bd3197d7b1c6c27553bc5618a27cce2/pyflakes-3.2.0.tar.gz", hash = "sha256:1c61603ff154621fb2a9172037d84dca3500def8c8b630657d1701f026f8af3f", size = 63788, upload-time = "2024-01-05T00:28:47.703Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d4/d7/f1b7db88d8e4417c5d47adad627a93547f44bdc9028372dbd2313f34a855/pyflakes-3.2.0-py2.py3-none-any.whl", hash = "sha256:84b5be138a2dfbb40689ca07e2152deb896a65c3a3e24c251c5c62489568074a", size = 62725, upload-time = "2024-01-05T00:28:45.903Z" },
        ]

        [tool]
        requirements = [{ name = "pyflakes" }]
        entrypoints = [
            { name = "pyflakes", install-path = "[TEMP_DIR]/bin/pyflakes", from = "pyflakes" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Install `pyflakes` as an editable package, but without any entrypoints.
    let pyflakes_local = context.temp_dir.child("pyflakes");
    fs_err::create_dir_all(pyflakes_local.path())?;

    let pyproject_toml = pyflakes_local.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "pyflakes"
        version = "0.1.0"
        description = "pyflakes without any entrypoints"
        authors = []
        dependencies = []
        requires-python = ">=3.11,<3.13"

        [build-system]
        requires = ["hatchling"]
        build-backend = "hatchling.build"
        "#
    })?;

    let src = pyflakes_local.child("src").child("pyflakes");
    fs_err::create_dir_all(src.path())?;

    let init = src.child("__init__.py");
    init.touch()?;

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("-e")
        .arg(pyflakes_local.path())
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----
    No executables are provided by package `pyflakes`; removing tool

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - pyflakes==3.2.0
     + pyflakes==0.1.0 (from file://[TEMP_DIR]/pyflakes)
    error: Failed to install entrypoints for `pyflakes`
    ");

    // Re-request `pyflakes`. It should reinstall, without requiring `--force`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("pyflakes")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + pyflakes==3.2.0
    Installed 1 executable: pyflakes
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("pyflakes").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "pyflakes" }]

        [[package]]
        name = "pyflakes"
        version = "3.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/57/f9/669d8c9c86613c9d568757c7f5824bd3197d7b1c6c27553bc5618a27cce2/pyflakes-3.2.0.tar.gz", hash = "sha256:1c61603ff154621fb2a9172037d84dca3500def8c8b630657d1701f026f8af3f", size = 63788, upload-time = "2024-01-05T00:28:47.703Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d4/d7/f1b7db88d8e4417c5d47adad627a93547f44bdc9028372dbd2313f34a855/pyflakes-3.2.0-py2.py3-none-any.whl", hash = "sha256:84b5be138a2dfbb40689ca07e2152deb896a65c3a3e24c251c5c62489568074a", size = 62725, upload-time = "2024-01-05T00:28:45.903Z" },
        ]

        [tool]
        requirements = [{ name = "pyflakes" }]
        entrypoints = [
            { name = "pyflakes", install-path = "[TEMP_DIR]/bin/pyflakes", from = "pyflakes" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    Ok(())
}

/// Test an editable installation of a tool using `--from`.
#[test]
fn tool_install_editable_from() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` as an editable package.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("-e")
        .arg("--from")
        .arg(context.workspace_root.join("test/packages/black_editable"))
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + black==0.1.0 (from file://[WORKSPACE]/test/packages/black_editable)
    Installed 1 executable: black
    ");

    tool_dir.child("black").assert(predicate::path::is_dir());
    tool_dir
        .child("black")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("black{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run black in the virtual environment
        assert_snapshot!(fs_err::read_to_string(&executable).unwrap(), @r#"
        #![TEMP_DIR]/tools/black/bin/python
        # -*- coding: utf-8 -*-
        import sys
        from black import main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(main())
        "#);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "black", editable = "[WORKSPACE]/test/packages/black_editable" }]

        [[package]]
        name = "black"
        version = "0.1.0"
        source = { editable = "[WORKSPACE]/test/packages/black_editable" }

        [package.metadata]
        requires-dist = [
            { name = "aiohttp", marker = "implementation_name == 'pypy' and sys_platform == 'win32' and extra == 'd'", specifier = ">=3.7.4,!=3.9.0" },
            { name = "aiohttp", marker = "(implementation_name != 'pypy' and extra == 'd') or (sys_platform != 'win32' and extra == 'd')", specifier = ">=3.7.4" },
            { name = "black", extras = ["d"], marker = "extra == 'dev'" },
            { name = "black", extras = ["uvloop"], marker = "extra == 'dev'" },
            { name = "colorama", marker = "extra == 'colorama'", specifier = ">=0.4.3" },
            { name = "ipython", marker = "extra == 'jupyter'", specifier = ">=7.8.0" },
            { name = "tokenize-rt", marker = "extra == 'jupyter'", specifier = ">=3.2.0" },
            { name = "uvloop", marker = "extra == 'uvloop'", specifier = ">=0.15.2" },
        ]
        provides-extras = ["colorama", "uvloop", "d", "jupyter", "dev"]

        [tool]
        requirements = [{ name = "black", editable = "[WORKSPACE]/test/packages/black_editable" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    uv_snapshot!(context.filters(), Command::new("black").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello world!

    ----- stderr -----
    ");
}

/// Test installing a tool with `uv tool install --from`
#[test]
fn tool_install_from() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` using `--from` to specify the version
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--from")
        .arg("black==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.2.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    // Attempt to install `black` using `--from` with a different package name
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--from")
        .arg("flask==24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Package name (`flask`) provided with `--from` does not match install request (`black`)
    ");

    // Attempt to install `black` using `--from` with a different version
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black==24.2.0")
        .arg("--from")
        .arg("black==24.3.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Package requirement (`black==24.3.0`) provided with `--from` conflicts with install request (`black==24.2.0`)
    ");
}

/// Test installing and reinstalling an already installed tool
#[test]
fn tool_install_already_installed() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    tool_dir.child("black").assert(predicate::path::is_dir());
    tool_dir
        .child("black")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("black{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run black in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r#"
        #![TEMP_DIR]/tools/black/bin/python
        # -*- coding: utf-8 -*-
        import sys
        from black import patched_main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(patched_main())
        "#);
    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "black" }]

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
        sdist = { url = "https://files.pythonhosted.org/packages/8f/5f/bac24a952668c7482cfdb4ebf91ba57a796c9da8829363a772040c1a3312/black-24.3.0.tar.gz", hash = "sha256:a0c9c4a0771afc6919578cec71ce82a3e31e054904e7197deacbc9382671c41f", size = 634292, upload-time = "2024-03-15T19:35:43.699Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b6/c6/1d174efa9ff02b22d0124c73fc5f4d4fb006d0d9a081aadc354d05754a13/black-24.3.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:2818cf72dfd5d289e48f37ccfa08b460bf469e67fb7c4abb07edc2e9f16fb63f", size = 1600822, upload-time = "2024-03-15T19:45:20.337Z" },
            { url = "https://files.pythonhosted.org/packages/d9/ed/704731afffe460b8ff0672623b40fce9fe569f2ee617c15857e4d4440a3a/black-24.3.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:4acf672def7eb1725f41f38bf6bf425c8237248bb0804faa3965c036f7672d11", size = 1429987, upload-time = "2024-03-15T19:45:00.637Z" },
            { url = "https://files.pythonhosted.org/packages/a8/05/8dd038e30caadab7120176d4bc109b7ca2f4457f12eef746b0560a583458/black-24.3.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:c7ed6668cbbfcd231fa0dc1b137d3e40c04c7f786e626b405c62bcd5db5857e4", size = 1755319, upload-time = "2024-03-15T19:38:24.009Z" },
            { url = "https://files.pythonhosted.org/packages/71/9d/e5fa1ff4ef1940be15a64883c0bb8d2fcf626efec996eab4ae5a8c691d2c/black-24.3.0-cp312-cp312-win_amd64.whl", hash = "sha256:56f52cfbd3dabe2798d76dbdd299faa046a901041faf2cf33288bc4e6dae57b5", size = 1385180, upload-time = "2024-03-15T19:39:37.014Z" },
            { url = "https://files.pythonhosted.org/packages/4d/ea/31770a7e49f3eedfd8cd7b35e78b3a3aaad860400f8673994bc988318135/black-24.3.0-py3-none-any.whl", hash = "sha256:41622020d7120e01d377f74249e677039d20e6344ff5851de8a10f11f513bf93", size = 201493, upload-time = "2024-03-15T19:35:41.572Z" },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121, upload-time = "2023-08-17T17:29:11.868Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941, upload-time = "2023-08-17T17:29:10.08Z" },
        ]

        [[package]]
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433, upload-time = "2023-02-04T12:11:27.157Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695, upload-time = "2023-02-04T12:11:25.002Z" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055, upload-time = "2024-01-31T01:00:36.02Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717, upload-time = "2024-01-31T01:00:34.019Z" },
        ]

        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Install `black` again
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    `black` is already installed
    ");

    tool_dir.child("black").assert(predicate::path::is_dir());
    bin_dir
        .child(format!("black{}", std::env::consts::EXE_SUFFIX))
        .assert(predicate::path::exists());

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should not have an additional tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "black" }]

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
        sdist = { url = "https://files.pythonhosted.org/packages/8f/5f/bac24a952668c7482cfdb4ebf91ba57a796c9da8829363a772040c1a3312/black-24.3.0.tar.gz", hash = "sha256:a0c9c4a0771afc6919578cec71ce82a3e31e054904e7197deacbc9382671c41f", size = 634292, upload-time = "2024-03-15T19:35:43.699Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b6/c6/1d174efa9ff02b22d0124c73fc5f4d4fb006d0d9a081aadc354d05754a13/black-24.3.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:2818cf72dfd5d289e48f37ccfa08b460bf469e67fb7c4abb07edc2e9f16fb63f", size = 1600822, upload-time = "2024-03-15T19:45:20.337Z" },
            { url = "https://files.pythonhosted.org/packages/d9/ed/704731afffe460b8ff0672623b40fce9fe569f2ee617c15857e4d4440a3a/black-24.3.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:4acf672def7eb1725f41f38bf6bf425c8237248bb0804faa3965c036f7672d11", size = 1429987, upload-time = "2024-03-15T19:45:00.637Z" },
            { url = "https://files.pythonhosted.org/packages/a8/05/8dd038e30caadab7120176d4bc109b7ca2f4457f12eef746b0560a583458/black-24.3.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:c7ed6668cbbfcd231fa0dc1b137d3e40c04c7f786e626b405c62bcd5db5857e4", size = 1755319, upload-time = "2024-03-15T19:38:24.009Z" },
            { url = "https://files.pythonhosted.org/packages/71/9d/e5fa1ff4ef1940be15a64883c0bb8d2fcf626efec996eab4ae5a8c691d2c/black-24.3.0-cp312-cp312-win_amd64.whl", hash = "sha256:56f52cfbd3dabe2798d76dbdd299faa046a901041faf2cf33288bc4e6dae57b5", size = 1385180, upload-time = "2024-03-15T19:39:37.014Z" },
            { url = "https://files.pythonhosted.org/packages/4d/ea/31770a7e49f3eedfd8cd7b35e78b3a3aaad860400f8673994bc988318135/black-24.3.0-py3-none-any.whl", hash = "sha256:41622020d7120e01d377f74249e677039d20e6344ff5851de8a10f11f513bf93", size = 201493, upload-time = "2024-03-15T19:35:41.572Z" },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121, upload-time = "2023-08-17T17:29:11.868Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941, upload-time = "2023-08-17T17:29:10.08Z" },
        ]

        [[package]]
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433, upload-time = "2023-02-04T12:11:27.157Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695, upload-time = "2023-02-04T12:11:25.002Z" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055, upload-time = "2024-01-31T01:00:36.02Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717, upload-time = "2024-01-31T01:00:34.019Z" },
        ]

        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });
}

/// Test installing a tool with a valid legacy receipt that lacks a lock.
#[test]
fn tool_install_migrates_lockless_receipt() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    context
        .tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str())
        .assert()
        .success();

    let receipt_path = tool_dir.child("black").child("uv-receipt.toml");
    let receipt = fs_err::read_to_string(&receipt_path)?;
    let (_, tool_receipt) = receipt
        .split_once("\n[tool]\n")
        .context("expected the tool receipt to contain a tool section")?;
    receipt_path.write_str(&format!("[tool]\n{tool_receipt}"))?;
    let lockless_receipt = fs_err::read_to_string(&receipt_path)?;

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(lockless_receipt, @r#"
        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Checked [N] packages in [TIME]
    Installed 2 executables: black, blackd
    ");

    let receipt = fs_err::read_to_string(&receipt_path)?;
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(receipt, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "black" }]

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
        sdist = { url = "https://files.pythonhosted.org/packages/8f/5f/bac24a952668c7482cfdb4ebf91ba57a796c9da8829363a772040c1a3312/black-24.3.0.tar.gz", hash = "sha256:a0c9c4a0771afc6919578cec71ce82a3e31e054904e7197deacbc9382671c41f", size = 634292, upload-time = "2024-03-15T19:35:43.699Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b6/c6/1d174efa9ff02b22d0124c73fc5f4d4fb006d0d9a081aadc354d05754a13/black-24.3.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:2818cf72dfd5d289e48f37ccfa08b460bf469e67fb7c4abb07edc2e9f16fb63f", size = 1600822, upload-time = "2024-03-15T19:45:20.337Z" },
            { url = "https://files.pythonhosted.org/packages/d9/ed/704731afffe460b8ff0672623b40fce9fe569f2ee617c15857e4d4440a3a/black-24.3.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:4acf672def7eb1725f41f38bf6bf425c8237248bb0804faa3965c036f7672d11", size = 1429987, upload-time = "2024-03-15T19:45:00.637Z" },
            { url = "https://files.pythonhosted.org/packages/a8/05/8dd038e30caadab7120176d4bc109b7ca2f4457f12eef746b0560a583458/black-24.3.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:c7ed6668cbbfcd231fa0dc1b137d3e40c04c7f786e626b405c62bcd5db5857e4", size = 1755319, upload-time = "2024-03-15T19:38:24.009Z" },
            { url = "https://files.pythonhosted.org/packages/71/9d/e5fa1ff4ef1940be15a64883c0bb8d2fcf626efec996eab4ae5a8c691d2c/black-24.3.0-cp312-cp312-win_amd64.whl", hash = "sha256:56f52cfbd3dabe2798d76dbdd299faa046a901041faf2cf33288bc4e6dae57b5", size = 1385180, upload-time = "2024-03-15T19:39:37.014Z" },
            { url = "https://files.pythonhosted.org/packages/4d/ea/31770a7e49f3eedfd8cd7b35e78b3a3aaad860400f8673994bc988318135/black-24.3.0-py3-none-any.whl", hash = "sha256:41622020d7120e01d377f74249e677039d20e6344ff5851de8a10f11f513bf93", size = 201493, upload-time = "2024-03-15T19:35:41.572Z" },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121, upload-time = "2023-08-17T17:29:11.868Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941, upload-time = "2023-08-17T17:29:10.08Z" },
        ]

        [[package]]
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433, upload-time = "2023-02-04T12:11:27.157Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695, upload-time = "2023-02-04T12:11:25.002Z" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055, upload-time = "2024-01-31T01:00:36.02Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717, upload-time = "2024-01-31T01:00:34.019Z" },
        ]

        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    Ok(())
}

/// Test migrating a legacy receipt without upgrading dependencies beyond the installed environment.
#[test]
fn tool_install_migrates_lockless_receipt_with_installed_preferences() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    context
        .tool_install()
        .arg("black==24.2.0")
        .arg("--exclude-newer")
        .arg("3 weeks")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2024-03-22T00:00:00Z")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str())
        .assert()
        .success();

    let receipt_path = tool_dir.child("black").child("uv-receipt.toml");
    let receipt = fs_err::read_to_string(&receipt_path)?;
    let (_, tool_receipt) = receipt
        .split_once("\n[tool]\n")
        .context("expected the tool receipt to contain a tool section")?;
    receipt_path.write_str(&format!("[tool]\n{tool_receipt}"))?;

    context
        .tool_install()
        .arg("black==24.2.0")
        .arg("--exclude-newer")
        .arg("3 weeks")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2024-04-15T00:00:00Z")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str())
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black==24.2.0")
        .arg("--exclude-newer")
        .arg("3 weeks")
        .env_remove(EnvVars::UV_EXCLUDE_NEWER)
        .env(EnvVars::UV_TEST_CURRENT_TIMESTAMP, "2024-04-15T00:00:00Z")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    `black==24.2.0` is already installed
    ");

    Ok(())
}

#[test]
fn tool_install_reinstall() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    // Install `black` again with the `--reinstall` flag
    // We should recreate the entire environment and reinstall the entry points
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--reinstall")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     ~ black==24.3.0
     ~ click==8.1.7
     ~ mypy-extensions==1.0.0
     ~ packaging==24.0
     ~ pathspec==0.12.1
     ~ platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    // Install `black` again with `--reinstall-package` for `black`
    // We should reinstall `black` in the environment and reinstall the entry points
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--reinstall-package")
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     ~ black==24.3.0
    Installed 2 executables: black, blackd
    ");

    // Install `black` again with `--reinstall-package` for a dependency
    // We should reinstall `click` in the environment but not reinstall `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--reinstall-package")
        .arg("click")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     ~ click==8.1.7
    Installed 2 executables: black, blackd
    ");
}

/// Test installing a tool when its entry point already exists
#[test]
fn tool_install_force() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let executable = bin_dir.child(format!("pyflakes{}", std::env::consts::EXE_SUFFIX));
    executable.touch().unwrap();

    // Attempt to install `pyflakes`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("pyflakes")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + pyflakes==3.2.0
    error: Executable already exists: pyflakes (use `--force` to overwrite)
    ");

    // We should delete the virtual environment
    assert!(!tool_dir.child("pyflakes").exists());

    // We should not write a tools entry
    assert!(!tool_dir.join("pyflakes").join("uv-receipt.toml").exists());

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Nor should we change the `pyflakes` entry point that exists
        assert_snapshot!(fs_err::read_to_string(&executable).unwrap(), @"");

    });

    // Attempt to install `pyflakes` with the `--reinstall` flag
    // Should have no effect
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("pyflakes")
        .arg("--reinstall")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + pyflakes==3.2.0
    error: Executable already exists: pyflakes (use `--force` to overwrite)
    ");

    // We should not create a virtual environment
    assert!(!tool_dir.child("pyflakes").exists());

    // We should not write a tools entry
    assert!(!tool_dir.join("tools.toml").exists());

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Nor should we change the `pyflakes` entry point that exists
        assert_snapshot!(fs_err::read_to_string(&executable).unwrap(), @"");

    });

    // Install `pyflakes` with `--force`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("pyflakes")
        .arg("--force")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + pyflakes==3.2.0
    Installed 1 executable: pyflakes
    ");

    tool_dir.child("pyflakes").assert(predicate::path::is_dir());

    let marker = tool_dir.child("pyflakes").child("marker");
    fs_err::write(&marker, b"marker").unwrap();
    marker.assert(predicate::path::is_file());

    // Re-install `pyflakes` with `--force`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("pyflakes")
        .arg("--force")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + pyflakes==3.2.0
    Installed 1 executable: pyflakes
    ");

    tool_dir.child("pyflakes").assert(predicate::path::is_dir());
    marker.assert(predicate::path::missing());

    // Re-install `pyflakes` without `--force`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("pyflakes")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    `pyflakes` is already installed
    ");

    tool_dir.child("pyflakes").assert(predicate::path::is_dir());

    // Re-install `pyflakes` with `--reinstall`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("pyflakes")
        .arg("--reinstall")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     ~ pyflakes==3.2.0
    Installed 1 executable: pyflakes
    ");

    tool_dir.child("pyflakes").assert(predicate::path::is_dir());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run pyflakes in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r#"
        #![TEMP_DIR]/tools/pyflakes/bin/python3
        # -*- coding: utf-8 -*-
        import sys
        from pyflakes.api import main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(main())
        "#);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("pyflakes").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "pyflakes" }]

        [[package]]
        name = "pyflakes"
        version = "3.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/57/f9/669d8c9c86613c9d568757c7f5824bd3197d7b1c6c27553bc5618a27cce2/pyflakes-3.2.0.tar.gz", hash = "sha256:1c61603ff154621fb2a9172037d84dca3500def8c8b630657d1701f026f8af3f", size = 63788, upload-time = "2024-01-05T00:28:47.703Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d4/d7/f1b7db88d8e4417c5d47adad627a93547f44bdc9028372dbd2313f34a855/pyflakes-3.2.0-py2.py3-none-any.whl", hash = "sha256:84b5be138a2dfbb40689ca07e2152deb896a65c3a3e24c251c5c62489568074a", size = 62725, upload-time = "2024-01-05T00:28:45.903Z" },
        ]

        [tool]
        requirements = [{ name = "pyflakes" }]
        entrypoints = [
            { name = "pyflakes", install-path = "[TEMP_DIR]/bin/pyflakes", from = "pyflakes" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });
}

/// Test `uv tool install` when the bin directory is inferred from `$HOME`
///
/// Only tested on Linux right now because it's not clear how to change the %USERPROFILE% on Windows
#[cfg(unix)]
#[test]
fn tool_install_home() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");

    // Install `black`
    let mut cmd = context.tool_install();
    cmd.arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(
            EnvVars::XDG_DATA_HOME,
            context.home_dir.child(".local").child("share").as_os_str(),
        )
        .env(
            EnvVars::PATH,
            context.home_dir.child(".local").child("bin").as_os_str(),
        );
    uv_snapshot!(context.filters(), cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    context
        .home_dir
        .child(format!(".local/bin/black{}", std::env::consts::EXE_SUFFIX))
        .assert(predicate::path::exists());
}

/// Test `uv tool install` when the bin directory is inferred from `$XDG_DATA_HOME`
#[test]
fn tool_install_xdg_data_home() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let data_home = context.temp_dir.child("data/home");
    let bin_dir = context.temp_dir.child("data/bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_DATA_HOME, data_home.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    context
        .temp_dir
        .child(format!("data/bin/black{}", std::env::consts::EXE_SUFFIX))
        .assert(predicate::path::exists());
}

/// Test `uv tool install` when the bin directory is set by `$XDG_BIN_HOME`
#[test]
fn tool_install_xdg_bin_home() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    bin_dir
        .child(format!("black{}", std::env::consts::EXE_SUFFIX))
        .assert(predicate::path::exists());
}

/// Test `uv tool install` when the bin directory is set by `$UV_TOOL_BIN_DIR`
#[test]
fn tool_install_tool_bin_dir() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::UV_TOOL_BIN_DIR, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    bin_dir
        .child(format!("black{}", std::env::consts::EXE_SUFFIX))
        .assert(predicate::path::exists());
}

/// Test installing a tool that lacks entrypoints
#[test]
fn tool_install_no_entrypoints() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("iniconfig")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----
    No executables are provided by package `iniconfig`; removing tool

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + iniconfig==2.0.0
    error: Failed to install entrypoints for `iniconfig`
    ");

    // Ensure the tool environment is not created.
    tool_dir
        .child("iniconfig")
        .assert(predicate::path::missing());
    bin_dir
        .child("iniconfig")
        .assert(predicate::path::missing());
}

/// Test installing a package that can't be installed.
#[test]
fn tool_install_uninstallable() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let filters = context
        .filters()
        .into_iter()
        .chain([
            (r"bdist\.[^/\\\s]+(-[^/\\\s]+)?", "bdist.linux-x86_64"),
            (r"\\\.", ""),
            (r"#+", "#"),
            (
                "Please read the installation instructions at:\n ",
                "Please read the installation instructions at:\n",
            ),
        ])
        .collect::<Vec<_>>();
    uv_snapshot!(filters, context.tool_install()
        .arg("pyenv")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
      × Failed to build `pyenv==0.0.1`
      ├─▶ The build backend returned an error
      ╰─▶ Call to `setuptools.build_meta:__legacy__.build_wheel` failed (exit status: 1)

          [stdout]
          running bdist_wheel
          running build
          installing to build/bdist.linux-x86_64/wheel
          running install

          [stderr]
          # NOTE #
          We are sorry, but this package is not installable with pip.

          Please read the installation instructions at:

          https://github.com/pyenv/pyenv#installation
          #


          hint: This usually indicates a problem with the package or the build environment.
    ");

    // Ensure the tool environment is not created.
    tool_dir.child("pyenv").assert(predicate::path::missing());
    bin_dir.child("pyenv").assert(predicate::path::missing());
}

/// Test installing a tool with a bare URL requirement.
#[test]
fn tool_install_unnamed_package() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("https://files.pythonhosted.org/packages/0f/89/294c9a6b6c75a08da55e9d05321d0707e9418735e3062b12ef0f54c33474/black-24.4.2-py3-none-any.whl")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.4.2 (from https://files.pythonhosted.org/packages/0f/89/294c9a6b6c75a08da55e9d05321d0707e9418735e3062b12ef0f54c33474/black-24.4.2-py3-none-any.whl)
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    tool_dir.child("black").assert(predicate::path::is_dir());
    tool_dir
        .child("black")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("black{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run black in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r#"
        #![TEMP_DIR]/tools/black/bin/python
        # -*- coding: utf-8 -*-
        import sys
        from black import patched_main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(patched_main())
        "#);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "black", url = "https://files.pythonhosted.org/packages/0f/89/294c9a6b6c75a08da55e9d05321d0707e9418735e3062b12ef0f54c33474/black-24.4.2-py3-none-any.whl" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    uv_snapshot!(context.filters(), Command::new("black").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.4.2 (compiled: no)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    ");
}

/// Test installing a tool with a Git requirement.
#[test]
#[cfg(feature = "test-git")]
fn tool_install_git() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");
    let mut paths = BTreeSet::new();

    // Avoid removing `git` from PATH
    let git_path = which::which("git")
        .expect("Failed to find `git` executable.")
        .parent()
        .expect("Failed to find `git` executable directory.")
        .to_path_buf();
    paths.insert(bin_dir.to_path_buf());
    paths.insert(git_path);
    // Git Submodule in macos seems to rely on `sed`.
    if cfg!(target_os = "macos") {
        let sed_path = which::which("sed")
            .expect("Failed to find `sed` executable.")
            .parent()
            .expect("Failed to find `sed` executable directory.")
            .to_path_buf();
        paths.insert(sed_path);
    }
    let path = std::env::join_paths(paths).unwrap();

    // Unnamed Git Install
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("git+https://github.com/psf/black@24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, path.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.2.0 (from git+https://github.com/psf/black@6fdf8a4af28071ed1d079c01122b34c5d587207a)
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    tool_dir.child("black").assert(predicate::path::is_dir());
    tool_dir
        .child("black")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("black{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    fs_err::remove_dir_all(&bin_dir).expect("Failed to remove bin dir.");
    fs_err::remove_dir_all(&tool_dir).expect("Failed to remove tool dir.");

    // Named Git Install
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black @ git+https://github.com/psf/black@24.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, path.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.2.0 (from git+https://github.com/psf/black@6fdf8a4af28071ed1d079c01122b34c5d587207a)
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    tool_dir.child("black").assert(predicate::path::is_dir());
    tool_dir
        .child("black")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("black{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());
}

/// Test installing a tool with a Git LFS enabled requirement.
#[test]
#[cfg(feature = "test-git-lfs")]
fn tool_install_git_lfs() {
    let context = uv_test::test_context!("3.13")
        .with_filtered_exe_suffix()
        .with_git_lfs_config();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");
    let mut paths = BTreeSet::new();

    // Avoid removing `git` or `git-lfs` from PATH
    let git_path = which::which("git")
        .expect("Failed to find `git` executable.")
        .parent()
        .expect("Failed to find `git` executable directory.")
        .to_path_buf();
    let git_lfs_path = which::which("git-lfs")
        .expect("Failed to find `git-lfs` executable.")
        .parent()
        .expect("Failed to find `git-lfs` executable directory.")
        .to_path_buf();
    paths.insert(bin_dir.to_path_buf());
    paths.insert(git_path);
    paths.insert(git_lfs_path);
    // Git LFS filter-process in macos seems to rely on `sh`.
    // Git Submodule in macos seems to rely on `sed`.
    if cfg!(target_os = "macos") {
        for bin_path in ["sh", "sed"].into_iter().map(|name| {
            which::which(name)
                .unwrap_or_else(|_| panic!("Failed to find `{name}` executable."))
                .parent()
                .unwrap_or_else(|| panic!("Failed to find `{name}` executable directory."))
                .to_path_buf()
        }) {
            paths.insert(bin_path);
        }
    }
    let path = std::env::join_paths(paths).unwrap();

    // Verify a successful LFS request
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--lfs")
        .arg("test-lfs-repo @ git+https://github.com/astral-sh/test-lfs-repo@e282f5be233e3f1d44934164895a043fc534b8aa")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, path.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + test-lfs-repo==0.1.0 (from git+https://github.com/astral-sh/test-lfs-repo@e282f5be233e3f1d44934164895a043fc534b8aa#lfs=true)
    Installed 2 executables: test-lfs-repo, test-lfs-repo-assets
    ");

    tool_dir
        .child("test-lfs-repo")
        .assert(predicate::path::is_dir());
    tool_dir
        .child("test-lfs-repo")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("test-lfs-repo{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("test-lfs-repo").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.13.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "test-lfs-repo", git = "https://github.com/astral-sh/test-lfs-repo?lfs=true&rev=e282f5be233e3f1d44934164895a043fc534b8aa" }]

        [[package]]
        name = "test-lfs-repo"
        version = "0.1.0"
        source = { git = "https://github.com/astral-sh/test-lfs-repo?lfs=true&rev=e282f5be233e3f1d44934164895a043fc534b8aa#e282f5be233e3f1d44934164895a043fc534b8aa" }

        [tool]
        requirements = [{ name = "test-lfs-repo", git = "https://github.com/astral-sh/test-lfs-repo?lfs=true&rev=e282f5be233e3f1d44934164895a043fc534b8aa" }]
        entrypoints = [
            { name = "test-lfs-repo", install-path = "[TEMP_DIR]/bin/test-lfs-repo", from = "test-lfs-repo" },
            { name = "test-lfs-repo-assets", install-path = "[TEMP_DIR]/bin/test-lfs-repo-assets", from = "test-lfs-repo" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    uv_snapshot!(context.filters(), Command::new("test-lfs-repo").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello from test-lfs-repo!

    ----- stderr -----
    ");

    uv_snapshot!(context.filters(), Command::new("test-lfs-repo-assets").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello from test-lfs-repo! LFS_TEST=True ANOTHER_LFS_TEST=True

    ----- stderr -----
    ");

    // Attempt to install when LFS artifacts are missing and LFS is requested.

    // The filters below will remove any boilerplate before what we actually want to match.
    // They help handle slightly different output in uv-distribution/src/source/mod.rs between
    // calls to `git` and `git_metadata` functions which don't have guaranteed execution order.
    // In addition, we can get different error codes depending on where the failure occurs,
    // although we know the error code cannot be 0.
    let context = context
        .with_filter((r"exit_code: -?[1-9]\d*", "exit_code: [ERROR_CODE]"))
        .with_filter((
            "(?s)(----- stderr -----).*?The source distribution `[^`]+` is missing Git LFS artifacts.*",
            "$1\n[PREFIX]The source distribution `[DISTRIBUTION]` is missing Git LFS artifacts",
        ));

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--reinstall")
        .arg("--lfs")
        .arg("test-lfs-repo @ git+https://github.com/astral-sh/test-lfs-repo@e282f5be233e3f1d44934164895a043fc534b8aa")
        .env(EnvVars::UV_INTERNAL__TEST_LFS_DISABLED, "1")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, path.as_os_str()), @"
    success: false
    exit_code: [ERROR_CODE]
    ----- stdout -----

    ----- stderr -----
    [PREFIX]The source distribution `[DISTRIBUTION]` is missing Git LFS artifacts
    ");

    // Attempt to install when LFS artifacts are missing but LFS was not requested.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("test-lfs-repo @ git+https://github.com/astral-sh/test-lfs-repo@e282f5be233e3f1d44934164895a043fc534b8aa")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, path.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     - test-lfs-repo==0.1.0 (from git+https://github.com/astral-sh/test-lfs-repo@e282f5be233e3f1d44934164895a043fc534b8aa#lfs=true)
     + test-lfs-repo==0.1.0 (from git+https://github.com/astral-sh/test-lfs-repo@e282f5be233e3f1d44934164895a043fc534b8aa)
    Installed 2 executables: test-lfs-repo, test-lfs-repo-assets
    ");

    #[cfg(not(windows))]
    uv_snapshot!(context.filters(), Command::new("test-lfs-repo-assets").env(EnvVars::PATH, bin_dir.as_os_str()), @r#"
    success: false
    exit_code: [ERROR_CODE]
    ----- stdout -----

    ----- stderr -----
    Traceback (most recent call last):
      File "[TEMP_DIR]/bin/test-lfs-repo-assets", line 10, in <module>
        sys.exit(main_lfs())
                 ~~~~~~~~^^
      File "[TEMP_DIR]/tools/test-lfs-repo/[PYTHON-LIB]/site-packages/test_lfs_repo/__init__.py", line 5, in main_lfs
        from .lfs_module import LFS_TEST
      File "[TEMP_DIR]/tools/test-lfs-repo/[PYTHON-LIB]/site-packages/test_lfs_repo/lfs_module.py", line 1
        version https://git-lfs.github.com/spec/v1
                ^^^^^
    SyntaxError: invalid syntax
    "#);

    #[cfg(windows)]
    uv_snapshot!(context.filters(), Command::new("test-lfs-repo-assets").env(EnvVars::PATH, bin_dir.as_os_str()), @r#"
    success: false
    exit_code: [ERROR_CODE]
    ----- stdout -----

    ----- stderr -----
    Traceback (most recent call last):
      File "<frozen runpy>", line 198, in _run_module_as_main
      File "<frozen runpy>", line 88, in _run_code
      File "[TEMP_DIR]/bin/test-lfs-repo-assets/__main__.py", line 10, in <module>
        sys.exit(main_lfs())
                 ~~~~~~~~^^
      File "[TEMP_DIR]/tools/test-lfs-repo/[PYTHON-LIB]/site-packages/test_lfs_repo/__init__.py", line 5, in main_lfs
        from .lfs_module import LFS_TEST
      File "[TEMP_DIR]/tools/test-lfs-repo/[PYTHON-LIB]/site-packages/test_lfs_repo/lfs_module.py", line 1
        version https://git-lfs.github.com/spec/v1
                ^^^^^
    SyntaxError: invalid syntax
    "#);
}

/// Test installing a tool with a bare URL requirement using `--from`, where the URL and the package
/// name conflict.
#[test]
fn tool_install_unnamed_conflict() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--from")
        .arg("https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Package name (`iniconfig`) provided with `--from` does not match install request (`black`)
    ");
}

/// Test installing a tool with a bare URL requirement using `--from`.
#[test]
fn tool_install_unnamed_from() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--from")
        .arg("https://files.pythonhosted.org/packages/0f/89/294c9a6b6c75a08da55e9d05321d0707e9418735e3062b12ef0f54c33474/black-24.4.2-py3-none-any.whl")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 6 packages in [TIME]
    Prepared 6 packages in [TIME]
    Installed 6 packages in [TIME]
     + black==24.4.2 (from https://files.pythonhosted.org/packages/0f/89/294c9a6b6c75a08da55e9d05321d0707e9418735e3062b12ef0f54c33474/black-24.4.2-py3-none-any.whl)
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    tool_dir.child("black").assert(predicate::path::is_dir());
    tool_dir
        .child("black")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("black{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run black in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r#"
        #![TEMP_DIR]/tools/black/bin/python
        # -*- coding: utf-8 -*-
        import sys
        from black import patched_main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(patched_main())
        "#);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [{ name = "black", url = "https://files.pythonhosted.org/packages/0f/89/294c9a6b6c75a08da55e9d05321d0707e9418735e3062b12ef0f54c33474/black-24.4.2-py3-none-any.whl" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    uv_snapshot!(context.filters(), Command::new("black").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.4.2 (compiled: no)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    ");
}

/// Test installing a tool with a bare URL requirement using `--with`.
#[test]
fn tool_install_unnamed_with() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--with")
        .arg("https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + iniconfig==2.0.0 (from https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl)
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    tool_dir.child("black").assert(predicate::path::is_dir());
    tool_dir
        .child("black")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("black{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run black in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r#"
        #![TEMP_DIR]/tools/black/bin/python
        # -*- coding: utf-8 -*-
        import sys
        from black import patched_main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(patched_main())
        "#);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [
            { name = "black" },
            { name = "iniconfig", url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl" },
        ]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    uv_snapshot!(context.filters(), Command::new("black").arg("--version").env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    black, 24.3.0 (compiled: yes)
    Python (CPython) 3.12.[X]

    ----- stderr -----
    ");
}

#[test]
fn tool_install_with_dependencies_from_script() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let script = context.temp_dir.child("script.py");
    script.write_str(indoc! {r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "anyio",
        # ]
        # ///

        import anyio
    "#})?;

    // script dependencies (anyio) are now installed.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("--with-requirements")
        .arg("script.py")
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + anyio==4.3.0
     + black==24.3.0
     + click==8.1.7
     + idna==3.6
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
     + sniffio==1.3.1
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [
            { name = "anyio" },
            { name = "black" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/8f/5f/bac24a952668c7482cfdb4ebf91ba57a796c9da8829363a772040c1a3312/black-24.3.0.tar.gz", hash = "sha256:a0c9c4a0771afc6919578cec71ce82a3e31e054904e7197deacbc9382671c41f", size = 634292, upload-time = "2024-03-15T19:35:43.699Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b6/c6/1d174efa9ff02b22d0124c73fc5f4d4fb006d0d9a081aadc354d05754a13/black-24.3.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:2818cf72dfd5d289e48f37ccfa08b460bf469e67fb7c4abb07edc2e9f16fb63f", size = 1600822, upload-time = "2024-03-15T19:45:20.337Z" },
            { url = "https://files.pythonhosted.org/packages/d9/ed/704731afffe460b8ff0672623b40fce9fe569f2ee617c15857e4d4440a3a/black-24.3.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:4acf672def7eb1725f41f38bf6bf425c8237248bb0804faa3965c036f7672d11", size = 1429987, upload-time = "2024-03-15T19:45:00.637Z" },
            { url = "https://files.pythonhosted.org/packages/a8/05/8dd038e30caadab7120176d4bc109b7ca2f4457f12eef746b0560a583458/black-24.3.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:c7ed6668cbbfcd231fa0dc1b137d3e40c04c7f786e626b405c62bcd5db5857e4", size = 1755319, upload-time = "2024-03-15T19:38:24.009Z" },
            { url = "https://files.pythonhosted.org/packages/71/9d/e5fa1ff4ef1940be15a64883c0bb8d2fcf626efec996eab4ae5a8c691d2c/black-24.3.0-cp312-cp312-win_amd64.whl", hash = "sha256:56f52cfbd3dabe2798d76dbdd299faa046a901041faf2cf33288bc4e6dae57b5", size = 1385180, upload-time = "2024-03-15T19:39:37.014Z" },
            { url = "https://files.pythonhosted.org/packages/4d/ea/31770a7e49f3eedfd8cd7b35e78b3a3aaad860400f8673994bc988318135/black-24.3.0-py3-none-any.whl", hash = "sha256:41622020d7120e01d377f74249e677039d20e6344ff5851de8a10f11f513bf93", size = 201493, upload-time = "2024-03-15T19:35:41.572Z" },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121, upload-time = "2023-08-17T17:29:11.868Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941, upload-time = "2023-08-17T17:29:10.08Z" },
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
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433, upload-time = "2023-02-04T12:11:27.157Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695, upload-time = "2023-02-04T12:11:25.002Z" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055, upload-time = "2024-01-31T01:00:36.02Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717, upload-time = "2024-01-31T01:00:34.019Z" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372, upload-time = "2024-02-25T23:20:04.057Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235, upload-time = "2024-02-25T23:20:01.196Z" },
        ]

        [tool]
        requirements = [
            { name = "black" },
            { name = "anyio" },
        ]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Update the script file.
    script.write_str(indoc! {r#"
        # /// script
        # requires-python = ">=3.11"
        # dependencies = [
        #   "anyio",
        #   "iniconfig",
        # ]
        # ///

        import anyio
    "#})?;

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("black")
        .arg("--with-requirements")
        .arg("script.py")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + iniconfig==2.0.0
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [
            { name = "anyio" },
            { name = "black" },
            { name = "iniconfig" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/8f/5f/bac24a952668c7482cfdb4ebf91ba57a796c9da8829363a772040c1a3312/black-24.3.0.tar.gz", hash = "sha256:a0c9c4a0771afc6919578cec71ce82a3e31e054904e7197deacbc9382671c41f", size = 634292, upload-time = "2024-03-15T19:35:43.699Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b6/c6/1d174efa9ff02b22d0124c73fc5f4d4fb006d0d9a081aadc354d05754a13/black-24.3.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:2818cf72dfd5d289e48f37ccfa08b460bf469e67fb7c4abb07edc2e9f16fb63f", size = 1600822, upload-time = "2024-03-15T19:45:20.337Z" },
            { url = "https://files.pythonhosted.org/packages/d9/ed/704731afffe460b8ff0672623b40fce9fe569f2ee617c15857e4d4440a3a/black-24.3.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:4acf672def7eb1725f41f38bf6bf425c8237248bb0804faa3965c036f7672d11", size = 1429987, upload-time = "2024-03-15T19:45:00.637Z" },
            { url = "https://files.pythonhosted.org/packages/a8/05/8dd038e30caadab7120176d4bc109b7ca2f4457f12eef746b0560a583458/black-24.3.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:c7ed6668cbbfcd231fa0dc1b137d3e40c04c7f786e626b405c62bcd5db5857e4", size = 1755319, upload-time = "2024-03-15T19:38:24.009Z" },
            { url = "https://files.pythonhosted.org/packages/71/9d/e5fa1ff4ef1940be15a64883c0bb8d2fcf626efec996eab4ae5a8c691d2c/black-24.3.0-cp312-cp312-win_amd64.whl", hash = "sha256:56f52cfbd3dabe2798d76dbdd299faa046a901041faf2cf33288bc4e6dae57b5", size = 1385180, upload-time = "2024-03-15T19:39:37.014Z" },
            { url = "https://files.pythonhosted.org/packages/4d/ea/31770a7e49f3eedfd8cd7b35e78b3a3aaad860400f8673994bc988318135/black-24.3.0-py3-none-any.whl", hash = "sha256:41622020d7120e01d377f74249e677039d20e6344ff5851de8a10f11f513bf93", size = 201493, upload-time = "2024-03-15T19:35:41.572Z" },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121, upload-time = "2023-08-17T17:29:11.868Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941, upload-time = "2023-08-17T17:29:10.08Z" },
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
        name = "iniconfig"
        version = "2.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646, upload-time = "2023-01-07T11:08:11.254Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892, upload-time = "2023-01-07T11:08:09.864Z" },
        ]

        [[package]]
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433, upload-time = "2023-02-04T12:11:27.157Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695, upload-time = "2023-02-04T12:11:25.002Z" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055, upload-time = "2024-01-31T01:00:36.02Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717, upload-time = "2024-01-31T01:00:34.019Z" },
        ]

        [[package]]
        name = "sniffio"
        version = "1.3.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372, upload-time = "2024-02-25T23:20:04.057Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235, upload-time = "2024-02-25T23:20:01.196Z" },
        ]

        [tool]
        requirements = [
            { name = "black" },
            { name = "anyio" },
            { name = "iniconfig" },
        ]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    Ok(())
}

/// Test installing a tool with additional requirements from a `requirements.txt` file.
#[test]
fn tool_install_requirements_txt() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("iniconfig").unwrap();

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("black")
        .arg("--with-requirements")
        .arg("requirements.txt")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + iniconfig==2.0.0
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [
            { name = "black" },
            { name = "iniconfig" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/8f/5f/bac24a952668c7482cfdb4ebf91ba57a796c9da8829363a772040c1a3312/black-24.3.0.tar.gz", hash = "sha256:a0c9c4a0771afc6919578cec71ce82a3e31e054904e7197deacbc9382671c41f", size = 634292, upload-time = "2024-03-15T19:35:43.699Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b6/c6/1d174efa9ff02b22d0124c73fc5f4d4fb006d0d9a081aadc354d05754a13/black-24.3.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:2818cf72dfd5d289e48f37ccfa08b460bf469e67fb7c4abb07edc2e9f16fb63f", size = 1600822, upload-time = "2024-03-15T19:45:20.337Z" },
            { url = "https://files.pythonhosted.org/packages/d9/ed/704731afffe460b8ff0672623b40fce9fe569f2ee617c15857e4d4440a3a/black-24.3.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:4acf672def7eb1725f41f38bf6bf425c8237248bb0804faa3965c036f7672d11", size = 1429987, upload-time = "2024-03-15T19:45:00.637Z" },
            { url = "https://files.pythonhosted.org/packages/a8/05/8dd038e30caadab7120176d4bc109b7ca2f4457f12eef746b0560a583458/black-24.3.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:c7ed6668cbbfcd231fa0dc1b137d3e40c04c7f786e626b405c62bcd5db5857e4", size = 1755319, upload-time = "2024-03-15T19:38:24.009Z" },
            { url = "https://files.pythonhosted.org/packages/71/9d/e5fa1ff4ef1940be15a64883c0bb8d2fcf626efec996eab4ae5a8c691d2c/black-24.3.0-cp312-cp312-win_amd64.whl", hash = "sha256:56f52cfbd3dabe2798d76dbdd299faa046a901041faf2cf33288bc4e6dae57b5", size = 1385180, upload-time = "2024-03-15T19:39:37.014Z" },
            { url = "https://files.pythonhosted.org/packages/4d/ea/31770a7e49f3eedfd8cd7b35e78b3a3aaad860400f8673994bc988318135/black-24.3.0-py3-none-any.whl", hash = "sha256:41622020d7120e01d377f74249e677039d20e6344ff5851de8a10f11f513bf93", size = 201493, upload-time = "2024-03-15T19:35:41.572Z" },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121, upload-time = "2023-08-17T17:29:11.868Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941, upload-time = "2023-08-17T17:29:10.08Z" },
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
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433, upload-time = "2023-02-04T12:11:27.157Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695, upload-time = "2023-02-04T12:11:25.002Z" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055, upload-time = "2024-01-31T01:00:36.02Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717, upload-time = "2024-01-31T01:00:34.019Z" },
        ]

        [tool]
        requirements = [
            { name = "black" },
            { name = "iniconfig" },
        ]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Update the `requirements.txt` file.
    requirements_txt.write_str("idna").unwrap();

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("black")
        .arg("--with-requirements")
        .arg("requirements.txt")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + idna==3.6
     - iniconfig==2.0.0
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [
            { name = "black" },
            { name = "idna" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/8f/5f/bac24a952668c7482cfdb4ebf91ba57a796c9da8829363a772040c1a3312/black-24.3.0.tar.gz", hash = "sha256:a0c9c4a0771afc6919578cec71ce82a3e31e054904e7197deacbc9382671c41f", size = 634292, upload-time = "2024-03-15T19:35:43.699Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b6/c6/1d174efa9ff02b22d0124c73fc5f4d4fb006d0d9a081aadc354d05754a13/black-24.3.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:2818cf72dfd5d289e48f37ccfa08b460bf469e67fb7c4abb07edc2e9f16fb63f", size = 1600822, upload-time = "2024-03-15T19:45:20.337Z" },
            { url = "https://files.pythonhosted.org/packages/d9/ed/704731afffe460b8ff0672623b40fce9fe569f2ee617c15857e4d4440a3a/black-24.3.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:4acf672def7eb1725f41f38bf6bf425c8237248bb0804faa3965c036f7672d11", size = 1429987, upload-time = "2024-03-15T19:45:00.637Z" },
            { url = "https://files.pythonhosted.org/packages/a8/05/8dd038e30caadab7120176d4bc109b7ca2f4457f12eef746b0560a583458/black-24.3.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:c7ed6668cbbfcd231fa0dc1b137d3e40c04c7f786e626b405c62bcd5db5857e4", size = 1755319, upload-time = "2024-03-15T19:38:24.009Z" },
            { url = "https://files.pythonhosted.org/packages/71/9d/e5fa1ff4ef1940be15a64883c0bb8d2fcf626efec996eab4ae5a8c691d2c/black-24.3.0-cp312-cp312-win_amd64.whl", hash = "sha256:56f52cfbd3dabe2798d76dbdd299faa046a901041faf2cf33288bc4e6dae57b5", size = 1385180, upload-time = "2024-03-15T19:39:37.014Z" },
            { url = "https://files.pythonhosted.org/packages/4d/ea/31770a7e49f3eedfd8cd7b35e78b3a3aaad860400f8673994bc988318135/black-24.3.0-py3-none-any.whl", hash = "sha256:41622020d7120e01d377f74249e677039d20e6344ff5851de8a10f11f513bf93", size = 201493, upload-time = "2024-03-15T19:35:41.572Z" },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121, upload-time = "2023-08-17T17:29:11.868Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941, upload-time = "2023-08-17T17:29:10.08Z" },
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
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433, upload-time = "2023-02-04T12:11:27.157Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695, upload-time = "2023-02-04T12:11:25.002Z" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055, upload-time = "2024-01-31T01:00:36.02Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717, upload-time = "2024-01-31T01:00:34.019Z" },
        ]

        [tool]
        requirements = [
            { name = "black" },
            { name = "idna" },
        ]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });
}

/// Ignore and warn when (e.g.) the `--index-url` argument is a provided `requirements.txt`.
#[test]
fn tool_install_requirements_txt_arguments() {
    let context = uv_test::test_context!("3.12").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str(indoc! { r"
        --index-url https://test.pypi.org/simple
        idna
        "
        })
        .unwrap();

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("black")
        .arg("--with-requirements")
        .arg("requirements.txt")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Ignoring `--index-url` from requirements file: `https://test.pypi.org/simple`. Instead, use the `--index-url` command-line argument, or set `index-url` in a `uv.toml` or `pyproject.toml` file.
    Resolved 7 packages in [TIME]
    Prepared 7 packages in [TIME]
    Installed 7 packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + idna==3.6
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [
            { name = "black" },
            { name = "idna" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/8f/5f/bac24a952668c7482cfdb4ebf91ba57a796c9da8829363a772040c1a3312/black-24.3.0.tar.gz", hash = "sha256:a0c9c4a0771afc6919578cec71ce82a3e31e054904e7197deacbc9382671c41f", size = 634292, upload-time = "2024-03-15T19:35:43.699Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b6/c6/1d174efa9ff02b22d0124c73fc5f4d4fb006d0d9a081aadc354d05754a13/black-24.3.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:2818cf72dfd5d289e48f37ccfa08b460bf469e67fb7c4abb07edc2e9f16fb63f", size = 1600822, upload-time = "2024-03-15T19:45:20.337Z" },
            { url = "https://files.pythonhosted.org/packages/d9/ed/704731afffe460b8ff0672623b40fce9fe569f2ee617c15857e4d4440a3a/black-24.3.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:4acf672def7eb1725f41f38bf6bf425c8237248bb0804faa3965c036f7672d11", size = 1429987, upload-time = "2024-03-15T19:45:00.637Z" },
            { url = "https://files.pythonhosted.org/packages/a8/05/8dd038e30caadab7120176d4bc109b7ca2f4457f12eef746b0560a583458/black-24.3.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:c7ed6668cbbfcd231fa0dc1b137d3e40c04c7f786e626b405c62bcd5db5857e4", size = 1755319, upload-time = "2024-03-15T19:38:24.009Z" },
            { url = "https://files.pythonhosted.org/packages/71/9d/e5fa1ff4ef1940be15a64883c0bb8d2fcf626efec996eab4ae5a8c691d2c/black-24.3.0-cp312-cp312-win_amd64.whl", hash = "sha256:56f52cfbd3dabe2798d76dbdd299faa046a901041faf2cf33288bc4e6dae57b5", size = 1385180, upload-time = "2024-03-15T19:39:37.014Z" },
            { url = "https://files.pythonhosted.org/packages/4d/ea/31770a7e49f3eedfd8cd7b35e78b3a3aaad860400f8673994bc988318135/black-24.3.0-py3-none-any.whl", hash = "sha256:41622020d7120e01d377f74249e677039d20e6344ff5851de8a10f11f513bf93", size = 201493, upload-time = "2024-03-15T19:35:41.572Z" },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121, upload-time = "2023-08-17T17:29:11.868Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941, upload-time = "2023-08-17T17:29:10.08Z" },
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
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433, upload-time = "2023-02-04T12:11:27.157Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695, upload-time = "2023-02-04T12:11:25.002Z" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055, upload-time = "2024-01-31T01:00:36.02Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717, upload-time = "2024-01-31T01:00:34.019Z" },
        ]

        [tool]
        requirements = [
            { name = "black" },
            { name = "idna" },
        ]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Don't warn, though, if the index URL is the same as the default or as settings.
    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str(indoc! { r"
        --index-url https://pypi.org/simple
        idna
        "
        })
        .unwrap();

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("black")
        .arg("--with-requirements")
        .arg("requirements.txt")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    `black` is already installed
    ");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt
        .write_str(indoc! { r"
        --index-url https://test.pypi.org/simple
        idna
        "
        })
        .unwrap();

    // Install `flask`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("flask")
        .arg("--with-requirements")
        .arg("requirements.txt")
        .arg("--index-url")
        .arg("https://test.pypi.org/simple")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    Prepared 8 packages in [TIME]
    Installed 8 packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.2
     + idna==2.7
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    Installed 1 executable: flask
    ");
}

/// Test upgrading an already installed tool.
#[test]
fn tool_install_upgrade() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("black==24.1.1")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.1.1
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "black", specifier = "==24.1.1" }]

        [[package]]
        name = "black"
        version = "24.1.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "click" },
            { name = "mypy-extensions" },
            { name = "packaging" },
            { name = "pathspec" },
            { name = "platformdirs" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/77/ec/a429d15d2e7f996203bff98e2b2e84ad4cb3de318de147b0038dc93fbc71/black-24.1.1.tar.gz", hash = "sha256:48b5760dcbfe5cf97fd4fba23946681f3a81514c6ab8a45b50da67ac8fbc6c7b", size = 623755, upload-time = "2024-01-28T05:28:48.365Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/17/9e/104321dd49d30f7e9475afef76db7ad14b43f56933a315a657504d8fbdd7/black-24.1.1-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:e2c8dfa14677f90d976f68e0c923947ae68fa3961d61ee30976c388adc0b02c8", size = 1567927, upload-time = "2024-01-28T05:43:39.588Z" },
            { url = "https://files.pythonhosted.org/packages/be/ff/9380fb957347ab897543b53228cfd85112e421bdaf243e3865fa2d5e80ce/black-24.1.1-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:a21725862d0e855ae05da1dd25e3825ed712eaaccef6b03017fe0853a01aa45e", size = 1397655, upload-time = "2024-01-28T05:39:08.418Z" },
            { url = "https://files.pythonhosted.org/packages/55/14/07a41fb78fe81aa4852f16af4211fab5a130fcd3150b44a336042a3252d5/black-24.1.1-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:07204d078e25327aad9ed2c64790d681238686bce254c910de640c7cc4fc3aa6", size = 1718031, upload-time = "2024-01-28T05:31:22.398Z" },
            { url = "https://files.pythonhosted.org/packages/e5/fa/eaa2c165840a2496654366fcdc17f63459b89e3296b9269a18ba6d71f596/black-24.1.1-cp312-cp312-win_amd64.whl", hash = "sha256:a83fe522d9698d8f9a101b860b1ee154c1d25f8a82ceb807d319f085b2627c5b", size = 1350588, upload-time = "2024-01-28T05:32:22.839Z" },
            { url = "https://files.pythonhosted.org/packages/95/f3/c3d59ae490c627950efc97a27c3f73776577e2ec32d35737e72aee3d6738/black-24.1.1-py3-none-any.whl", hash = "sha256:5cdc2e2195212208fbcae579b931407c1fa9997584f0a415421748aeafff1168", size = 195702, upload-time = "2024-01-28T05:28:45.636Z" },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121, upload-time = "2023-08-17T17:29:11.868Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941, upload-time = "2023-08-17T17:29:10.08Z" },
        ]

        [[package]]
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433, upload-time = "2023-02-04T12:11:27.157Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695, upload-time = "2023-02-04T12:11:25.002Z" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055, upload-time = "2024-01-31T01:00:36.02Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717, upload-time = "2024-01-31T01:00:34.019Z" },
        ]

        [tool]
        requirements = [{ name = "black", specifier = "==24.1.1" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Install without the constraint. It should be replaced, but the package shouldn't be installed
    // since it's already satisfied in the environment.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Checked [N] packages in [TIME]
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "black" }]

        [[package]]
        name = "black"
        version = "24.1.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "click" },
            { name = "mypy-extensions" },
            { name = "packaging" },
            { name = "pathspec" },
            { name = "platformdirs" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/77/ec/a429d15d2e7f996203bff98e2b2e84ad4cb3de318de147b0038dc93fbc71/black-24.1.1.tar.gz", hash = "sha256:48b5760dcbfe5cf97fd4fba23946681f3a81514c6ab8a45b50da67ac8fbc6c7b", size = 623755, upload-time = "2024-01-28T05:28:48.365Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/17/9e/104321dd49d30f7e9475afef76db7ad14b43f56933a315a657504d8fbdd7/black-24.1.1-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:e2c8dfa14677f90d976f68e0c923947ae68fa3961d61ee30976c388adc0b02c8", size = 1567927, upload-time = "2024-01-28T05:43:39.588Z" },
            { url = "https://files.pythonhosted.org/packages/be/ff/9380fb957347ab897543b53228cfd85112e421bdaf243e3865fa2d5e80ce/black-24.1.1-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:a21725862d0e855ae05da1dd25e3825ed712eaaccef6b03017fe0853a01aa45e", size = 1397655, upload-time = "2024-01-28T05:39:08.418Z" },
            { url = "https://files.pythonhosted.org/packages/55/14/07a41fb78fe81aa4852f16af4211fab5a130fcd3150b44a336042a3252d5/black-24.1.1-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:07204d078e25327aad9ed2c64790d681238686bce254c910de640c7cc4fc3aa6", size = 1718031, upload-time = "2024-01-28T05:31:22.398Z" },
            { url = "https://files.pythonhosted.org/packages/e5/fa/eaa2c165840a2496654366fcdc17f63459b89e3296b9269a18ba6d71f596/black-24.1.1-cp312-cp312-win_amd64.whl", hash = "sha256:a83fe522d9698d8f9a101b860b1ee154c1d25f8a82ceb807d319f085b2627c5b", size = 1350588, upload-time = "2024-01-28T05:32:22.839Z" },
            { url = "https://files.pythonhosted.org/packages/95/f3/c3d59ae490c627950efc97a27c3f73776577e2ec32d35737e72aee3d6738/black-24.1.1-py3-none-any.whl", hash = "sha256:5cdc2e2195212208fbcae579b931407c1fa9997584f0a415421748aeafff1168", size = 195702, upload-time = "2024-01-28T05:28:45.636Z" },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121, upload-time = "2023-08-17T17:29:11.868Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941, upload-time = "2023-08-17T17:29:10.08Z" },
        ]

        [[package]]
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433, upload-time = "2023-02-04T12:11:27.157Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695, upload-time = "2023-02-04T12:11:25.002Z" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055, upload-time = "2024-01-31T01:00:36.02Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717, upload-time = "2024-01-31T01:00:34.019Z" },
        ]

        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Install with a `with`. It should be added to the environment.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("black")
        .arg("--with")
        .arg("iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + iniconfig==2.0.0 (from https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl)
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        [tool]
        requirements = [
            { name = "black" },
            { name = "iniconfig", url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl" },
        ]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Install with `--upgrade`. `black` should be reinstalled with a more recent version, and
    // `iniconfig` should be removed.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("black")
        .arg("--upgrade")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     - black==24.1.1
     + black==24.3.0
     - iniconfig==2.0.0 (from https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl)
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "black" }]

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
        sdist = { url = "https://files.pythonhosted.org/packages/8f/5f/bac24a952668c7482cfdb4ebf91ba57a796c9da8829363a772040c1a3312/black-24.3.0.tar.gz", hash = "sha256:a0c9c4a0771afc6919578cec71ce82a3e31e054904e7197deacbc9382671c41f", size = 634292, upload-time = "2024-03-15T19:35:43.699Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b6/c6/1d174efa9ff02b22d0124c73fc5f4d4fb006d0d9a081aadc354d05754a13/black-24.3.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:2818cf72dfd5d289e48f37ccfa08b460bf469e67fb7c4abb07edc2e9f16fb63f", size = 1600822, upload-time = "2024-03-15T19:45:20.337Z" },
            { url = "https://files.pythonhosted.org/packages/d9/ed/704731afffe460b8ff0672623b40fce9fe569f2ee617c15857e4d4440a3a/black-24.3.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:4acf672def7eb1725f41f38bf6bf425c8237248bb0804faa3965c036f7672d11", size = 1429987, upload-time = "2024-03-15T19:45:00.637Z" },
            { url = "https://files.pythonhosted.org/packages/a8/05/8dd038e30caadab7120176d4bc109b7ca2f4457f12eef746b0560a583458/black-24.3.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:c7ed6668cbbfcd231fa0dc1b137d3e40c04c7f786e626b405c62bcd5db5857e4", size = 1755319, upload-time = "2024-03-15T19:38:24.009Z" },
            { url = "https://files.pythonhosted.org/packages/71/9d/e5fa1ff4ef1940be15a64883c0bb8d2fcf626efec996eab4ae5a8c691d2c/black-24.3.0-cp312-cp312-win_amd64.whl", hash = "sha256:56f52cfbd3dabe2798d76dbdd299faa046a901041faf2cf33288bc4e6dae57b5", size = 1385180, upload-time = "2024-03-15T19:39:37.014Z" },
            { url = "https://files.pythonhosted.org/packages/4d/ea/31770a7e49f3eedfd8cd7b35e78b3a3aaad860400f8673994bc988318135/black-24.3.0-py3-none-any.whl", hash = "sha256:41622020d7120e01d377f74249e677039d20e6344ff5851de8a10f11f513bf93", size = 201493, upload-time = "2024-03-15T19:35:41.572Z" },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121, upload-time = "2023-08-17T17:29:11.868Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941, upload-time = "2023-08-17T17:29:10.08Z" },
        ]

        [[package]]
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433, upload-time = "2023-02-04T12:11:27.157Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695, upload-time = "2023-02-04T12:11:25.002Z" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055, upload-time = "2024-01-31T01:00:36.02Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717, upload-time = "2024-01-31T01:00:34.019Z" },
        ]

        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });
}

/// Test reinstalling tools with varying `--python` requests.
#[test]
fn tool_install_python_requests() {
    let context = uv_test::test_context_with_versions!(&["3.11", "3.12"])
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("-p")
        .arg("3.12")
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    // Install with Python 3.12 (compatible).
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("-p")
        .arg("3.12")
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    `black` is already installed
    ");

    // // Install with Python 3.11 (incompatible).
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("-p")
        .arg("3.11")
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Ignoring existing environment for `black`: the requested Python interpreter does not match the environment interpreter
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");
}

/// Test reinstalling tools with varying `--python` and
/// `--python-preference` parameters.
#[ignore = "https://github.com/astral-sh/uv/issues/7473"]
#[test]
fn tool_install_python_preference() {
    let context = uv_test::test_context_with_versions!(&["3.11", "3.12"])
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("-p")
        .arg("3.12")
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    "###);

    // Install with Python 3.12 (compatible).
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("-p")
        .arg("3.12")
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    `black` is already installed
    "###);

    // Install with system Python 3.11 (different version, incompatible).
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("-p")
        .arg("3.11")
        .arg("--python-preference")
        .arg("only-system")
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Ignoring existing environment for `black`: the requested Python interpreter does not match the environment interpreter
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    "###);

    // Install with system Python 3.11 (compatible).
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("-p")
        .arg("3.11")
        .arg("--python-preference")
        .arg("only-system")
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    `black` is already installed
    "###);

    // Install with managed Python 3.11 (different source, incompatible).
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("-p")
        .arg("3.11")
        .arg("--python-preference")
        .arg("only-managed")
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Ignoring existing environment for `black`: the requested Python interpreter does not match the environment interpreter
    Resolved [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    "###);

    // Install with managed Python 3.11 (compatible).
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("-p")
        .arg("3.11")
        .arg("--python-preference")
        .arg("only-managed")
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    `black` is already installed
    "###);
}

/// Test preserving a tool environment when new but incompatible requirements are requested.
#[test]
fn tool_install_preserve_environment() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black==24.1.1")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.1.1
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    // Install `black`, but with an incompatible requirement.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black==24.1.1")
        .arg("--with")
        .arg("packaging==0.0.1")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving dependencies:
      ╰─▶ Because black==24.1.1 depends on packaging>=22.0 and you require black==24.1.1, we can conclude that you require packaging>=22.0.
          And because you require packaging==0.0.1, we can conclude that your requirements are unsatisfiable.
    ");

    // Install `black`. The tool should already be installed, since we didn't remove the environment.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black==24.1.1")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    `black==24.1.1` is already installed
    ");
}

/// Test warning when the binary directory is not on the user's PATH.
#[test]
#[cfg(unix)]
fn tool_install_warn_path() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black==24.1.1")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env_remove(EnvVars::PATH), @r#"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.1.1
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    warning: `[TEMP_DIR]/bin` is not on your PATH. To use installed tools, run `export PATH="[TEMP_DIR]/bin:$PATH"` or `uv tool update-shell`.
    "#);
}

/// Test installing and reinstalling with an invalid receipt.
#[test]
fn tool_install_bad_receipt() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    tool_dir
        .child("black")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    // Override the `uv-receipt.toml` file with an invalid receipt.
    tool_dir
        .child("black")
        .child("uv-receipt.toml")
        .write_str("invalid")?;

    // Reinstall `black`, which should remove the invalid receipt.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Removed existing `black` with invalid receipt
    Resolved [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    Ok(())
}

/// Test installing a tool with a malformed `.dist-info` directory (i.e., a `.dist-info` directory
/// that isn't properly normalized).
#[test]
fn tool_install_malformed_dist_info() {
    let context = uv_test::test_context!("3.12")
        .with_exclude_newer("2025-01-18T00:00:00Z")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `executable-application`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("executable-application")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + executable-application==0.3.0
    Installed 1 executable: app
    ");

    tool_dir
        .child("executable-application")
        .assert(predicate::path::is_dir());
    tool_dir
        .child("executable-application")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("app{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run black in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r#"
        #![TEMP_DIR]/tools/executable-application/bin/python
        # -*- coding: utf-8 -*-
        import sys
        from executable_application import main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(main())
        "#);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("executable-application").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2025-01-18T00:00:00Z"

        [manifest]
        requirements = [{ name = "executable-application" }]

        [[package]]
        name = "executable-application"
        version = "0.3.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/9a/36/e803315469274d62f2dab543e3916c0b5b65730074d295f7d48711aa9e36/executable_application-0.3.0.tar.gz", hash = "sha256:0ef8c5ddd28649503c6e4a9f55be17e5b3bd0685df7b83ff7c260b481025f261", size = 914, upload-time = "2025-01-17T23:21:24.559Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/32/97/8ab6fa1bbcb0a888f460c0a19c301f4cc4180573564ad7dd98b5ceca2ab6/executable_application-0.3.0-py3-none-any.whl", hash = "sha256:ca272aee7332e9d266663bc70037cd3ef1d74ffae40030eaf9ca46462dc8dcc6", size = 1719, upload-time = "2025-01-17T23:21:22.716Z" },
        ]

        [tool]
        requirements = [{ name = "executable-application" }]
        entrypoints = [
            { name = "app", install-path = "[TEMP_DIR]/bin/app", from = "executable-application" },
        ]

        [tool.options]
        exclude-newer = "2025-01-18T00:00:00Z"
        "#);
    });
}

/// Test installing, then re-installing with different settings.
#[test]
fn tool_install_settings() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("flask>=3")
        .arg("--resolution=lowest-direct")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + blinker==1.7.0
     + click==8.1.7
     + flask==3.0.0
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + werkzeug==3.0.1
    Installed 1 executable: flask
    ");

    tool_dir.child("flask").assert(predicate::path::is_dir());
    tool_dir
        .child("flask")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("flask{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r#"
        #![TEMP_DIR]/tools/flask/bin/python
        # -*- coding: utf-8 -*-
        import sys
        from flask.cli import main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(main())
        "#);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("flask").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        resolution-mode = "lowest-direct"
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "flask", specifier = ">=3" }]

        [[package]]
        name = "blinker"
        version = "1.7.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a1/13/6df5fc090ff4e5d246baf1f45fe9e5623aa8565757dfa5bd243f6a545f9e/blinker-1.7.0.tar.gz", hash = "sha256:e6820ff6fa4e4d1d8e2747c2283749c3f547e4fee112b98555cdcdae32996182", size = 28134, upload-time = "2023-11-01T22:06:01.588Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/fa/2a/7f3714cbc6356a0efec525ce7a0613d581072ed6eb53eb7b9754f33db807/blinker-1.7.0-py3-none-any.whl", hash = "sha256:c3f865d4d54db7abc53758a01601cf343fe55b84c1de4e3fa910e420b438d5b9", size = 13068, upload-time = "2023-11-01T22:06:00.162Z" },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121, upload-time = "2023-08-17T17:29:11.868Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941, upload-time = "2023-08-17T17:29:10.08Z" },
        ]

        [[package]]
        name = "flask"
        version = "3.0.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "blinker" },
            { name = "click" },
            { name = "itsdangerous" },
            { name = "jinja2" },
            { name = "werkzeug" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/d8/09/c1a7354d3925a3c6c8cfdebf4245bae67d633ffda1ba415add06ffc839c5/flask-3.0.0.tar.gz", hash = "sha256:cfadcdb638b609361d29ec22360d6070a77d7463dcb3ab08d2c2f2f168845f58", size = 674171, upload-time = "2023-09-30T14:36:12.918Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/36/42/015c23096649b908c809c69388a805a571a3bea44362fe87e33fc3afa01f/flask-3.0.0-py3-none-any.whl", hash = "sha256:21128f47e4e3b9d597a3e8521a329bf56909b690fcc3fa3e477725aa81367638", size = 99724, upload-time = "2023-09-30T14:36:10.961Z" },
        ]

        [[package]]
        name = "itsdangerous"
        version = "2.1.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/7f/a1/d3fb83e7a61fa0c0d3d08ad0a94ddbeff3731c05212617dff3a94e097f08/itsdangerous-2.1.2.tar.gz", hash = "sha256:5dbbc68b317e5e42f327f9021763545dc3fc3bfe22e6deb96aaf1fc38874156a", size = 56143, upload-time = "2022-03-24T15:12:15.102Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/5f/447e04e828f47465eeab35b5d408b7ebaaaee207f48b7136c5a7267a30ae/itsdangerous-2.1.2-py3-none-any.whl", hash = "sha256:2c2349112351b88699d8d4b6b075022c0808887cb7ad10069318a8b0bc88db44", size = 15749, upload-time = "2022-03-24T15:12:13.2Z" },
        ]

        [[package]]
        name = "jinja2"
        version = "3.1.3"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "markupsafe" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b2/5e/3a21abf3cd467d7876045335e681d276ac32492febe6d98ad89562d1a7e1/Jinja2-3.1.3.tar.gz", hash = "sha256:ac8bd6544d4bb2c9792bf3a159e80bba8fda7f07e81bc3aed565432d5925ba90", size = 268261, upload-time = "2024-01-10T23:12:21.133Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/30/6d/6de6be2d02603ab56e72997708809e8a5b0fbfee080735109b40a3564843/Jinja2-3.1.3-py3-none-any.whl", hash = "sha256:7d6d50dd97d52cbc355597bd845fabfbac3f551e1f99619e39a35ce8c370b5fa", size = 133236, upload-time = "2024-01-10T23:12:19.504Z" },
        ]

        [[package]]
        name = "markupsafe"
        version = "2.1.5"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/87/5b/aae44c6655f3801e81aa3eef09dbbf012431987ba564d7231722f68df02d/MarkupSafe-2.1.5.tar.gz", hash = "sha256:d283d37a890ba4c1ae73ffadf8046435c76e7bc2247bbb63c00bd1a709c6544b", size = 19384, upload-time = "2024-02-02T16:31:22.863Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/53/bd/583bf3e4c8d6a321938c13f49d44024dbe5ed63e0a7ba127e454a66da974/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:8dec4936e9c3100156f8a2dc89c4b88d5c435175ff03413b443469c7c8c5f4d1", size = 18215, upload-time = "2024-02-02T16:30:33.081Z" },
            { url = "https://files.pythonhosted.org/packages/48/d6/e7cd795fc710292c3af3a06d80868ce4b02bfbbf370b7cee11d282815a2a/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:3c6b973f22eb18a789b1460b4b91bf04ae3f0c4234a0a6aa6b0a92f6f7b951d4", size = 14069, upload-time = "2024-02-02T16:30:34.148Z" },
            { url = "https://files.pythonhosted.org/packages/51/b5/5d8ec796e2a08fc814a2c7d2584b55f889a55cf17dd1a90f2beb70744e5c/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ac07bad82163452a6884fe8fa0963fb98c2346ba78d779ec06bd7a6262132aee", size = 29452, upload-time = "2024-02-02T16:30:35.149Z" },
            { url = "https://files.pythonhosted.org/packages/0a/0d/2454f072fae3b5a137c119abf15465d1771319dfe9e4acbb31722a0fff91/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f5dfb42c4604dddc8e4305050aa6deb084540643ed5804d7455b5df8fe16f5e5", size = 28462, upload-time = "2024-02-02T16:30:36.166Z" },
            { url = "https://files.pythonhosted.org/packages/2d/75/fd6cb2e68780f72d47e6671840ca517bda5ef663d30ada7616b0462ad1e3/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ea3d8a3d18833cf4304cd2fc9cbb1efe188ca9b5efef2bdac7adc20594a0e46b", size = 27869, upload-time = "2024-02-02T16:30:37.834Z" },
            { url = "https://files.pythonhosted.org/packages/b0/81/147c477391c2750e8fc7705829f7351cf1cd3be64406edcf900dc633feb2/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:d050b3361367a06d752db6ead6e7edeb0009be66bc3bae0ee9d97fb326badc2a", size = 33906, upload-time = "2024-02-02T16:30:39.366Z" },
            { url = "https://files.pythonhosted.org/packages/8b/ff/9a52b71839d7a256b563e85d11050e307121000dcebc97df120176b3ad93/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:bec0a414d016ac1a18862a519e54b2fd0fc8bbfd6890376898a6c0891dd82e9f", size = 32296, upload-time = "2024-02-02T16:30:40.413Z" },
            { url = "https://files.pythonhosted.org/packages/88/07/2dc76aa51b481eb96a4c3198894f38b480490e834479611a4053fbf08623/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:58c98fee265677f63a4385256a6d7683ab1832f3ddd1e66fe948d5880c21a169", size = 33038, upload-time = "2024-02-02T16:30:42.243Z" },
            { url = "https://files.pythonhosted.org/packages/96/0c/620c1fb3661858c0e37eb3cbffd8c6f732a67cd97296f725789679801b31/MarkupSafe-2.1.5-cp312-cp312-win32.whl", hash = "sha256:8590b4ae07a35970728874632fed7bd57b26b0102df2d2b233b6d9d82f6c62ad", size = 16572, upload-time = "2024-02-02T16:30:43.326Z" },
            { url = "https://files.pythonhosted.org/packages/3f/14/c3554d512d5f9100a95e737502f4a2323a1959f6d0d01e0d0997b35f7b10/MarkupSafe-2.1.5-cp312-cp312-win_amd64.whl", hash = "sha256:823b65d8706e32ad2df51ed89496147a42a2a6e01c13cfb6ffb8b1e92bc910bb", size = 17127, upload-time = "2024-02-02T16:30:44.418Z" },
        ]

        [[package]]
        name = "werkzeug"
        version = "3.0.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "markupsafe" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/0d/cc/ff1904eb5eb4b455e442834dabf9427331ac0fa02853bf83db817a7dd53d/werkzeug-3.0.1.tar.gz", hash = "sha256:507e811ecea72b18a404947aded4b3390e1db8f826b494d76550ef45bb3b1dcc", size = 801436, upload-time = "2023-10-24T20:57:50.084Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c3/fc/254c3e9b5feb89ff5b9076a23218dafbc99c96ac5941e900b71206e6313b/werkzeug-3.0.1-py3-none-any.whl", hash = "sha256:90a285dc0e42ad56b34e696398b8122ee4c681833fb35b8334a095d82c56da10", size = 226669, upload-time = "2023-10-24T20:57:47.326Z" },
        ]

        [tool]
        requirements = [{ name = "flask", specifier = ">=3" }]
        entrypoints = [
            { name = "flask", install-path = "[TEMP_DIR]/bin/flask", from = "flask" },
        ]

        [tool.options]
        resolution = "lowest-direct"
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Reinstall with `highest`. This is a no-op, since we _do_ have a compatible version installed.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("flask>=3")
        .arg("--resolution=highest")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    `flask>=3` is already installed
    ");

    // It should update the receipt though.
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("flask").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        resolution-mode = "lowest-direct"
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "flask", specifier = ">=3" }]

        [[package]]
        name = "blinker"
        version = "1.7.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a1/13/6df5fc090ff4e5d246baf1f45fe9e5623aa8565757dfa5bd243f6a545f9e/blinker-1.7.0.tar.gz", hash = "sha256:e6820ff6fa4e4d1d8e2747c2283749c3f547e4fee112b98555cdcdae32996182", size = 28134, upload-time = "2023-11-01T22:06:01.588Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/fa/2a/7f3714cbc6356a0efec525ce7a0613d581072ed6eb53eb7b9754f33db807/blinker-1.7.0-py3-none-any.whl", hash = "sha256:c3f865d4d54db7abc53758a01601cf343fe55b84c1de4e3fa910e420b438d5b9", size = 13068, upload-time = "2023-11-01T22:06:00.162Z" },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121, upload-time = "2023-08-17T17:29:11.868Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941, upload-time = "2023-08-17T17:29:10.08Z" },
        ]

        [[package]]
        name = "flask"
        version = "3.0.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "blinker" },
            { name = "click" },
            { name = "itsdangerous" },
            { name = "jinja2" },
            { name = "werkzeug" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/d8/09/c1a7354d3925a3c6c8cfdebf4245bae67d633ffda1ba415add06ffc839c5/flask-3.0.0.tar.gz", hash = "sha256:cfadcdb638b609361d29ec22360d6070a77d7463dcb3ab08d2c2f2f168845f58", size = 674171, upload-time = "2023-09-30T14:36:12.918Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/36/42/015c23096649b908c809c69388a805a571a3bea44362fe87e33fc3afa01f/flask-3.0.0-py3-none-any.whl", hash = "sha256:21128f47e4e3b9d597a3e8521a329bf56909b690fcc3fa3e477725aa81367638", size = 99724, upload-time = "2023-09-30T14:36:10.961Z" },
        ]

        [[package]]
        name = "itsdangerous"
        version = "2.1.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/7f/a1/d3fb83e7a61fa0c0d3d08ad0a94ddbeff3731c05212617dff3a94e097f08/itsdangerous-2.1.2.tar.gz", hash = "sha256:5dbbc68b317e5e42f327f9021763545dc3fc3bfe22e6deb96aaf1fc38874156a", size = 56143, upload-time = "2022-03-24T15:12:15.102Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/5f/447e04e828f47465eeab35b5d408b7ebaaaee207f48b7136c5a7267a30ae/itsdangerous-2.1.2-py3-none-any.whl", hash = "sha256:2c2349112351b88699d8d4b6b075022c0808887cb7ad10069318a8b0bc88db44", size = 15749, upload-time = "2022-03-24T15:12:13.2Z" },
        ]

        [[package]]
        name = "jinja2"
        version = "3.1.3"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "markupsafe" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b2/5e/3a21abf3cd467d7876045335e681d276ac32492febe6d98ad89562d1a7e1/Jinja2-3.1.3.tar.gz", hash = "sha256:ac8bd6544d4bb2c9792bf3a159e80bba8fda7f07e81bc3aed565432d5925ba90", size = 268261, upload-time = "2024-01-10T23:12:21.133Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/30/6d/6de6be2d02603ab56e72997708809e8a5b0fbfee080735109b40a3564843/Jinja2-3.1.3-py3-none-any.whl", hash = "sha256:7d6d50dd97d52cbc355597bd845fabfbac3f551e1f99619e39a35ce8c370b5fa", size = 133236, upload-time = "2024-01-10T23:12:19.504Z" },
        ]

        [[package]]
        name = "markupsafe"
        version = "2.1.5"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/87/5b/aae44c6655f3801e81aa3eef09dbbf012431987ba564d7231722f68df02d/MarkupSafe-2.1.5.tar.gz", hash = "sha256:d283d37a890ba4c1ae73ffadf8046435c76e7bc2247bbb63c00bd1a709c6544b", size = 19384, upload-time = "2024-02-02T16:31:22.863Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/53/bd/583bf3e4c8d6a321938c13f49d44024dbe5ed63e0a7ba127e454a66da974/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:8dec4936e9c3100156f8a2dc89c4b88d5c435175ff03413b443469c7c8c5f4d1", size = 18215, upload-time = "2024-02-02T16:30:33.081Z" },
            { url = "https://files.pythonhosted.org/packages/48/d6/e7cd795fc710292c3af3a06d80868ce4b02bfbbf370b7cee11d282815a2a/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:3c6b973f22eb18a789b1460b4b91bf04ae3f0c4234a0a6aa6b0a92f6f7b951d4", size = 14069, upload-time = "2024-02-02T16:30:34.148Z" },
            { url = "https://files.pythonhosted.org/packages/51/b5/5d8ec796e2a08fc814a2c7d2584b55f889a55cf17dd1a90f2beb70744e5c/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ac07bad82163452a6884fe8fa0963fb98c2346ba78d779ec06bd7a6262132aee", size = 29452, upload-time = "2024-02-02T16:30:35.149Z" },
            { url = "https://files.pythonhosted.org/packages/0a/0d/2454f072fae3b5a137c119abf15465d1771319dfe9e4acbb31722a0fff91/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f5dfb42c4604dddc8e4305050aa6deb084540643ed5804d7455b5df8fe16f5e5", size = 28462, upload-time = "2024-02-02T16:30:36.166Z" },
            { url = "https://files.pythonhosted.org/packages/2d/75/fd6cb2e68780f72d47e6671840ca517bda5ef663d30ada7616b0462ad1e3/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ea3d8a3d18833cf4304cd2fc9cbb1efe188ca9b5efef2bdac7adc20594a0e46b", size = 27869, upload-time = "2024-02-02T16:30:37.834Z" },
            { url = "https://files.pythonhosted.org/packages/b0/81/147c477391c2750e8fc7705829f7351cf1cd3be64406edcf900dc633feb2/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:d050b3361367a06d752db6ead6e7edeb0009be66bc3bae0ee9d97fb326badc2a", size = 33906, upload-time = "2024-02-02T16:30:39.366Z" },
            { url = "https://files.pythonhosted.org/packages/8b/ff/9a52b71839d7a256b563e85d11050e307121000dcebc97df120176b3ad93/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:bec0a414d016ac1a18862a519e54b2fd0fc8bbfd6890376898a6c0891dd82e9f", size = 32296, upload-time = "2024-02-02T16:30:40.413Z" },
            { url = "https://files.pythonhosted.org/packages/88/07/2dc76aa51b481eb96a4c3198894f38b480490e834479611a4053fbf08623/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:58c98fee265677f63a4385256a6d7683ab1832f3ddd1e66fe948d5880c21a169", size = 33038, upload-time = "2024-02-02T16:30:42.243Z" },
            { url = "https://files.pythonhosted.org/packages/96/0c/620c1fb3661858c0e37eb3cbffd8c6f732a67cd97296f725789679801b31/MarkupSafe-2.1.5-cp312-cp312-win32.whl", hash = "sha256:8590b4ae07a35970728874632fed7bd57b26b0102df2d2b233b6d9d82f6c62ad", size = 16572, upload-time = "2024-02-02T16:30:43.326Z" },
            { url = "https://files.pythonhosted.org/packages/3f/14/c3554d512d5f9100a95e737502f4a2323a1959f6d0d01e0d0997b35f7b10/MarkupSafe-2.1.5-cp312-cp312-win_amd64.whl", hash = "sha256:823b65d8706e32ad2df51ed89496147a42a2a6e01c13cfb6ffb8b1e92bc910bb", size = 17127, upload-time = "2024-02-02T16:30:44.418Z" },
        ]

        [[package]]
        name = "werkzeug"
        version = "3.0.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "markupsafe" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/0d/cc/ff1904eb5eb4b455e442834dabf9427331ac0fa02853bf83db817a7dd53d/werkzeug-3.0.1.tar.gz", hash = "sha256:507e811ecea72b18a404947aded4b3390e1db8f826b494d76550ef45bb3b1dcc", size = 801436, upload-time = "2023-10-24T20:57:50.084Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c3/fc/254c3e9b5feb89ff5b9076a23218dafbc99c96ac5941e900b71206e6313b/werkzeug-3.0.1-py3-none-any.whl", hash = "sha256:90a285dc0e42ad56b34e696398b8122ee4c681833fb35b8334a095d82c56da10", size = 226669, upload-time = "2023-10-24T20:57:47.326Z" },
        ]

        [tool]
        requirements = [{ name = "flask", specifier = ">=3" }]
        entrypoints = [
            { name = "flask", install-path = "[TEMP_DIR]/bin/flask", from = "flask" },
        ]

        [tool.options]
        resolution = "highest"
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Reinstall with `highest` and `--upgrade`. This should change the setting and install a higher
    // version.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("flask>=3")
        .arg("--resolution=highest")
        .arg("--upgrade")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     - flask==3.0.0
     + flask==3.0.2
    Installed 1 executable: flask
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("flask").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "flask", specifier = ">=3" }]

        [[package]]
        name = "blinker"
        version = "1.7.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/a1/13/6df5fc090ff4e5d246baf1f45fe9e5623aa8565757dfa5bd243f6a545f9e/blinker-1.7.0.tar.gz", hash = "sha256:e6820ff6fa4e4d1d8e2747c2283749c3f547e4fee112b98555cdcdae32996182", size = 28134, upload-time = "2023-11-01T22:06:01.588Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/fa/2a/7f3714cbc6356a0efec525ce7a0613d581072ed6eb53eb7b9754f33db807/blinker-1.7.0-py3-none-any.whl", hash = "sha256:c3f865d4d54db7abc53758a01601cf343fe55b84c1de4e3fa910e420b438d5b9", size = 13068, upload-time = "2023-11-01T22:06:00.162Z" },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121, upload-time = "2023-08-17T17:29:11.868Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941, upload-time = "2023-08-17T17:29:10.08Z" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/3f/e0/a89e8120faea1edbfca1a9b171cff7f2bf62ec860bbafcb2c2387c0317be/flask-3.0.2.tar.gz", hash = "sha256:822c03f4b799204250a7ee84b1eddc40665395333973dfb9deebfe425fefcb7d", size = 675248, upload-time = "2024-02-03T21:11:44.79Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/93/a6/aa98bfe0eb9b8b15d36cdfd03c8ca86a03968a87f27ce224fb4f766acb23/flask-3.0.2-py3-none-any.whl", hash = "sha256:3232e0e9c850d781933cf0207523d1ece087eb8d87b23777ae38456e2fbe7c6e", size = 101300, upload-time = "2024-02-03T21:11:42.661Z" },
        ]

        [[package]]
        name = "itsdangerous"
        version = "2.1.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/7f/a1/d3fb83e7a61fa0c0d3d08ad0a94ddbeff3731c05212617dff3a94e097f08/itsdangerous-2.1.2.tar.gz", hash = "sha256:5dbbc68b317e5e42f327f9021763545dc3fc3bfe22e6deb96aaf1fc38874156a", size = 56143, upload-time = "2022-03-24T15:12:15.102Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/68/5f/447e04e828f47465eeab35b5d408b7ebaaaee207f48b7136c5a7267a30ae/itsdangerous-2.1.2-py3-none-any.whl", hash = "sha256:2c2349112351b88699d8d4b6b075022c0808887cb7ad10069318a8b0bc88db44", size = 15749, upload-time = "2022-03-24T15:12:13.2Z" },
        ]

        [[package]]
        name = "jinja2"
        version = "3.1.3"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "markupsafe" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b2/5e/3a21abf3cd467d7876045335e681d276ac32492febe6d98ad89562d1a7e1/Jinja2-3.1.3.tar.gz", hash = "sha256:ac8bd6544d4bb2c9792bf3a159e80bba8fda7f07e81bc3aed565432d5925ba90", size = 268261, upload-time = "2024-01-10T23:12:21.133Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/30/6d/6de6be2d02603ab56e72997708809e8a5b0fbfee080735109b40a3564843/Jinja2-3.1.3-py3-none-any.whl", hash = "sha256:7d6d50dd97d52cbc355597bd845fabfbac3f551e1f99619e39a35ce8c370b5fa", size = 133236, upload-time = "2024-01-10T23:12:19.504Z" },
        ]

        [[package]]
        name = "markupsafe"
        version = "2.1.5"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/87/5b/aae44c6655f3801e81aa3eef09dbbf012431987ba564d7231722f68df02d/MarkupSafe-2.1.5.tar.gz", hash = "sha256:d283d37a890ba4c1ae73ffadf8046435c76e7bc2247bbb63c00bd1a709c6544b", size = 19384, upload-time = "2024-02-02T16:31:22.863Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/53/bd/583bf3e4c8d6a321938c13f49d44024dbe5ed63e0a7ba127e454a66da974/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:8dec4936e9c3100156f8a2dc89c4b88d5c435175ff03413b443469c7c8c5f4d1", size = 18215, upload-time = "2024-02-02T16:30:33.081Z" },
            { url = "https://files.pythonhosted.org/packages/48/d6/e7cd795fc710292c3af3a06d80868ce4b02bfbbf370b7cee11d282815a2a/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:3c6b973f22eb18a789b1460b4b91bf04ae3f0c4234a0a6aa6b0a92f6f7b951d4", size = 14069, upload-time = "2024-02-02T16:30:34.148Z" },
            { url = "https://files.pythonhosted.org/packages/51/b5/5d8ec796e2a08fc814a2c7d2584b55f889a55cf17dd1a90f2beb70744e5c/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ac07bad82163452a6884fe8fa0963fb98c2346ba78d779ec06bd7a6262132aee", size = 29452, upload-time = "2024-02-02T16:30:35.149Z" },
            { url = "https://files.pythonhosted.org/packages/0a/0d/2454f072fae3b5a137c119abf15465d1771319dfe9e4acbb31722a0fff91/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f5dfb42c4604dddc8e4305050aa6deb084540643ed5804d7455b5df8fe16f5e5", size = 28462, upload-time = "2024-02-02T16:30:36.166Z" },
            { url = "https://files.pythonhosted.org/packages/2d/75/fd6cb2e68780f72d47e6671840ca517bda5ef663d30ada7616b0462ad1e3/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ea3d8a3d18833cf4304cd2fc9cbb1efe188ca9b5efef2bdac7adc20594a0e46b", size = 27869, upload-time = "2024-02-02T16:30:37.834Z" },
            { url = "https://files.pythonhosted.org/packages/b0/81/147c477391c2750e8fc7705829f7351cf1cd3be64406edcf900dc633feb2/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:d050b3361367a06d752db6ead6e7edeb0009be66bc3bae0ee9d97fb326badc2a", size = 33906, upload-time = "2024-02-02T16:30:39.366Z" },
            { url = "https://files.pythonhosted.org/packages/8b/ff/9a52b71839d7a256b563e85d11050e307121000dcebc97df120176b3ad93/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:bec0a414d016ac1a18862a519e54b2fd0fc8bbfd6890376898a6c0891dd82e9f", size = 32296, upload-time = "2024-02-02T16:30:40.413Z" },
            { url = "https://files.pythonhosted.org/packages/88/07/2dc76aa51b481eb96a4c3198894f38b480490e834479611a4053fbf08623/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:58c98fee265677f63a4385256a6d7683ab1832f3ddd1e66fe948d5880c21a169", size = 33038, upload-time = "2024-02-02T16:30:42.243Z" },
            { url = "https://files.pythonhosted.org/packages/96/0c/620c1fb3661858c0e37eb3cbffd8c6f732a67cd97296f725789679801b31/MarkupSafe-2.1.5-cp312-cp312-win32.whl", hash = "sha256:8590b4ae07a35970728874632fed7bd57b26b0102df2d2b233b6d9d82f6c62ad", size = 16572, upload-time = "2024-02-02T16:30:43.326Z" },
            { url = "https://files.pythonhosted.org/packages/3f/14/c3554d512d5f9100a95e737502f4a2323a1959f6d0d01e0d0997b35f7b10/MarkupSafe-2.1.5-cp312-cp312-win_amd64.whl", hash = "sha256:823b65d8706e32ad2df51ed89496147a42a2a6e01c13cfb6ffb8b1e92bc910bb", size = 17127, upload-time = "2024-02-02T16:30:44.418Z" },
        ]

        [[package]]
        name = "werkzeug"
        version = "3.0.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "markupsafe" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/0d/cc/ff1904eb5eb4b455e442834dabf9427331ac0fa02853bf83db817a7dd53d/werkzeug-3.0.1.tar.gz", hash = "sha256:507e811ecea72b18a404947aded4b3390e1db8f826b494d76550ef45bb3b1dcc", size = 801436, upload-time = "2023-10-24T20:57:50.084Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/c3/fc/254c3e9b5feb89ff5b9076a23218dafbc99c96ac5941e900b71206e6313b/werkzeug-3.0.1-py3-none-any.whl", hash = "sha256:90a285dc0e42ad56b34e696398b8122ee4c681833fb35b8334a095d82c56da10", size = 226669, upload-time = "2023-10-24T20:57:47.326Z" },
        ]

        [tool]
        requirements = [{ name = "flask", specifier = ">=3" }]
        entrypoints = [
            { name = "flask", install-path = "[TEMP_DIR]/bin/flask", from = "flask" },
        ]

        [tool.options]
        resolution = "highest"
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });
}

/// Test installing a tool with `uv tool install {package}@{version}`.
#[test]
fn tool_install_at_version() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` at `24.1.0`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("black@24.1.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.1.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "black", specifier = "==24.1.0" }]

        [[package]]
        name = "black"
        version = "24.1.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "click" },
            { name = "mypy-extensions" },
            { name = "packaging" },
            { name = "pathspec" },
            { name = "platformdirs" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/ea/19/33d4f2f0babcbc07d3e2c058a64c76606cf19884a600536c837aaf4e4f2d/black-24.1.0.tar.gz", hash = "sha256:30fbf768cd4f4576598b1db0202413fafea9a227ef808d1a12230c643cefe9fc", size = 622911, upload-time = "2024-01-26T05:12:33.568Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/a4/36/4877583cc05d0152d119f74ccd39516b81593cdda3f3142e7b0254619fe9/black-24.1.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:d74d4d0da276fbe3b95aa1f404182562c28a04402e4ece60cf373d0b902f33a0", size = 1565463, upload-time = "2024-01-26T05:24:33.012Z" },
            { url = "https://files.pythonhosted.org/packages/4d/f6/a1b2d9d343bf9023e7471adc4e5505e3aa9b1d916ae43150dc550e2037db/black-24.1.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:39addf23f7070dbc0b5518cdb2018468ac249d7412a669b50ccca18427dba1f3", size = 1396556, upload-time = "2024-01-26T05:24:22.127Z" },
            { url = "https://files.pythonhosted.org/packages/50/b8/fc7fda91bfa12597241e952447422698197951aaa18d9c332aab7489748a/black-24.1.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:827a7c0da520dd2f8e6d7d3595f4591aa62ccccce95b16c0e94bb4066374c4c2", size = 1716439, upload-time = "2024-01-26T05:14:52.256Z" },
            { url = "https://files.pythonhosted.org/packages/07/32/39217587b93675832c8d06acbb2c6260e1e840ce495c0bfbfb750b6fd0ec/black-24.1.0-cp312-cp312-win_amd64.whl", hash = "sha256:0cd59d01bf3306ff7e3076dd7f4435fcd2fafe5506a6111cae1138fc7de52382", size = 1349450, upload-time = "2024-01-26T05:16:21.038Z" },
            { url = "https://files.pythonhosted.org/packages/49/69/cfd27026c25f49eb97d1e6992a8fada9b4a3f225e89ef361cd3a65462d84/black-24.1.0-py3-none-any.whl", hash = "sha256:5134a6f6b683aa0a5592e3fd61dd3519d8acd953d93e2b8b76f9981245b65594", size = 195345, upload-time = "2024-01-26T05:12:30.775Z" },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121, upload-time = "2023-08-17T17:29:11.868Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941, upload-time = "2023-08-17T17:29:10.08Z" },
        ]

        [[package]]
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433, upload-time = "2023-02-04T12:11:27.157Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695, upload-time = "2023-02-04T12:11:25.002Z" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055, upload-time = "2024-01-31T01:00:36.02Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717, upload-time = "2024-01-31T01:00:34.019Z" },
        ]

        [tool]
        requirements = [{ name = "black", specifier = "==24.1.0" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Combining `{package}@{version}` with a `--from` should fail (even if they're ultimately
    // compatible).
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("black@24.1.0")
        .arg("--from")
        .arg("black==24.1.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Package requirement (`black==24.1.0`) provided with `--from` conflicts with install request (`black@24.1.0`)
    ");
}

/// Test installing a tool with `uv tool install {package}@latest`.
#[test]
fn tool_install_at_latest() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` at latest.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("black@latest")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "black" }]

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
        sdist = { url = "https://files.pythonhosted.org/packages/8f/5f/bac24a952668c7482cfdb4ebf91ba57a796c9da8829363a772040c1a3312/black-24.3.0.tar.gz", hash = "sha256:a0c9c4a0771afc6919578cec71ce82a3e31e054904e7197deacbc9382671c41f", size = 634292, upload-time = "2024-03-15T19:35:43.699Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b6/c6/1d174efa9ff02b22d0124c73fc5f4d4fb006d0d9a081aadc354d05754a13/black-24.3.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:2818cf72dfd5d289e48f37ccfa08b460bf469e67fb7c4abb07edc2e9f16fb63f", size = 1600822, upload-time = "2024-03-15T19:45:20.337Z" },
            { url = "https://files.pythonhosted.org/packages/d9/ed/704731afffe460b8ff0672623b40fce9fe569f2ee617c15857e4d4440a3a/black-24.3.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:4acf672def7eb1725f41f38bf6bf425c8237248bb0804faa3965c036f7672d11", size = 1429987, upload-time = "2024-03-15T19:45:00.637Z" },
            { url = "https://files.pythonhosted.org/packages/a8/05/8dd038e30caadab7120176d4bc109b7ca2f4457f12eef746b0560a583458/black-24.3.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:c7ed6668cbbfcd231fa0dc1b137d3e40c04c7f786e626b405c62bcd5db5857e4", size = 1755319, upload-time = "2024-03-15T19:38:24.009Z" },
            { url = "https://files.pythonhosted.org/packages/71/9d/e5fa1ff4ef1940be15a64883c0bb8d2fcf626efec996eab4ae5a8c691d2c/black-24.3.0-cp312-cp312-win_amd64.whl", hash = "sha256:56f52cfbd3dabe2798d76dbdd299faa046a901041faf2cf33288bc4e6dae57b5", size = 1385180, upload-time = "2024-03-15T19:39:37.014Z" },
            { url = "https://files.pythonhosted.org/packages/4d/ea/31770a7e49f3eedfd8cd7b35e78b3a3aaad860400f8673994bc988318135/black-24.3.0-py3-none-any.whl", hash = "sha256:41622020d7120e01d377f74249e677039d20e6344ff5851de8a10f11f513bf93", size = 201493, upload-time = "2024-03-15T19:35:41.572Z" },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121, upload-time = "2023-08-17T17:29:11.868Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941, upload-time = "2023-08-17T17:29:10.08Z" },
        ]

        [[package]]
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433, upload-time = "2023-02-04T12:11:27.157Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695, upload-time = "2023-02-04T12:11:25.002Z" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055, upload-time = "2024-01-31T01:00:36.02Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717, upload-time = "2024-01-31T01:00:34.019Z" },
        ]

        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });
}

/// Test installing a tool with `uv tool install {package} --from {package}@latest`.
#[test]
fn tool_install_from_at_latest() {
    let context = uv_test::test_context!("3.12")
        .with_exclude_newer("2025-01-18T00:00:00Z")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("app")
        .arg("--from")
        .arg("executable-application@latest")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + executable-application==0.3.0
    Installed 1 executable: app
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("executable-application").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2025-01-18T00:00:00Z"

        [manifest]
        requirements = [{ name = "executable-application" }]

        [[package]]
        name = "executable-application"
        version = "0.3.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/9a/36/e803315469274d62f2dab543e3916c0b5b65730074d295f7d48711aa9e36/executable_application-0.3.0.tar.gz", hash = "sha256:0ef8c5ddd28649503c6e4a9f55be17e5b3bd0685df7b83ff7c260b481025f261", size = 914, upload-time = "2025-01-17T23:21:24.559Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/32/97/8ab6fa1bbcb0a888f460c0a19c301f4cc4180573564ad7dd98b5ceca2ab6/executable_application-0.3.0-py3-none-any.whl", hash = "sha256:ca272aee7332e9d266663bc70037cd3ef1d74ffae40030eaf9ca46462dc8dcc6", size = 1719, upload-time = "2025-01-17T23:21:22.716Z" },
        ]

        [tool]
        requirements = [{ name = "executable-application" }]
        entrypoints = [
            { name = "app", install-path = "[TEMP_DIR]/bin/app", from = "executable-application" },
        ]

        [tool.options]
        exclude-newer = "2025-01-18T00:00:00Z"
        "#);
    });
}

/// Test installing a tool with `uv tool install {package} --from {package}@{version}`.
#[test]
fn tool_install_from_at_version() {
    let context = uv_test::test_context!("3.12")
        .with_exclude_newer("2025-01-18T00:00:00Z")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("app")
        .arg("--from")
        .arg("executable-application@0.2.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + executable-application==0.2.0
    Installed 1 executable: app
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("executable-application").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2025-01-18T00:00:00Z"

        [manifest]
        requirements = [{ name = "executable-application", specifier = "==0.2.0" }]

        [[package]]
        name = "executable-application"
        version = "0.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/fb/cc/782db0ab8b2387abeacf554a9533c5b596c09f97c737de25dad1358325ae/executable_application-0.2.0.tar.gz", hash = "sha256:5c5e2ce5f44c842fb820b3256306deecf450ee076968f13b20ccfb811d31e777", size = 915, upload-time = "2025-01-17T23:21:10.508Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b7/d6/d9e5e20e5fd52a2ff02c4dcd351a2a2fb1e1a159c5de355aded16bccadef/executable_application-0.2.0-py3-none-any.whl", hash = "sha256:2b26f00eb59ebe606697535aee0bfcf1f7ae0dfa7223703cd40cf1c31959d149", size = 1718, upload-time = "2025-01-17T23:21:08.354Z" },
        ]

        [tool]
        requirements = [{ name = "executable-application", specifier = "==0.2.0" }]
        entrypoints = [
            { name = "app", install-path = "[TEMP_DIR]/bin/app", from = "executable-application" },
        ]

        [tool.options]
        exclude-newer = "2025-01-18T00:00:00Z"
        "#);
    });
}

/// Test upgrading an already installed tool via `{package}@{latest}`.
#[test]
fn tool_install_at_latest_upgrade() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("black==24.1.1")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.1.1
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "black", specifier = "==24.1.1" }]

        [[package]]
        name = "black"
        version = "24.1.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "click" },
            { name = "mypy-extensions" },
            { name = "packaging" },
            { name = "pathspec" },
            { name = "platformdirs" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/77/ec/a429d15d2e7f996203bff98e2b2e84ad4cb3de318de147b0038dc93fbc71/black-24.1.1.tar.gz", hash = "sha256:48b5760dcbfe5cf97fd4fba23946681f3a81514c6ab8a45b50da67ac8fbc6c7b", size = 623755, upload-time = "2024-01-28T05:28:48.365Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/17/9e/104321dd49d30f7e9475afef76db7ad14b43f56933a315a657504d8fbdd7/black-24.1.1-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:e2c8dfa14677f90d976f68e0c923947ae68fa3961d61ee30976c388adc0b02c8", size = 1567927, upload-time = "2024-01-28T05:43:39.588Z" },
            { url = "https://files.pythonhosted.org/packages/be/ff/9380fb957347ab897543b53228cfd85112e421bdaf243e3865fa2d5e80ce/black-24.1.1-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:a21725862d0e855ae05da1dd25e3825ed712eaaccef6b03017fe0853a01aa45e", size = 1397655, upload-time = "2024-01-28T05:39:08.418Z" },
            { url = "https://files.pythonhosted.org/packages/55/14/07a41fb78fe81aa4852f16af4211fab5a130fcd3150b44a336042a3252d5/black-24.1.1-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:07204d078e25327aad9ed2c64790d681238686bce254c910de640c7cc4fc3aa6", size = 1718031, upload-time = "2024-01-28T05:31:22.398Z" },
            { url = "https://files.pythonhosted.org/packages/e5/fa/eaa2c165840a2496654366fcdc17f63459b89e3296b9269a18ba6d71f596/black-24.1.1-cp312-cp312-win_amd64.whl", hash = "sha256:a83fe522d9698d8f9a101b860b1ee154c1d25f8a82ceb807d319f085b2627c5b", size = 1350588, upload-time = "2024-01-28T05:32:22.839Z" },
            { url = "https://files.pythonhosted.org/packages/95/f3/c3d59ae490c627950efc97a27c3f73776577e2ec32d35737e72aee3d6738/black-24.1.1-py3-none-any.whl", hash = "sha256:5cdc2e2195212208fbcae579b931407c1fa9997584f0a415421748aeafff1168", size = 195702, upload-time = "2024-01-28T05:28:45.636Z" },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121, upload-time = "2023-08-17T17:29:11.868Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941, upload-time = "2023-08-17T17:29:10.08Z" },
        ]

        [[package]]
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433, upload-time = "2023-02-04T12:11:27.157Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695, upload-time = "2023-02-04T12:11:25.002Z" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055, upload-time = "2024-01-31T01:00:36.02Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717, upload-time = "2024-01-31T01:00:34.019Z" },
        ]

        [tool]
        requirements = [{ name = "black", specifier = "==24.1.1" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Install without the constraint. It should be replaced, but the package shouldn't be installed
    // since it's already satisfied in the environment.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("black")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Checked [N] packages in [TIME]
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "black" }]

        [[package]]
        name = "black"
        version = "24.1.1"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "click" },
            { name = "mypy-extensions" },
            { name = "packaging" },
            { name = "pathspec" },
            { name = "platformdirs" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/77/ec/a429d15d2e7f996203bff98e2b2e84ad4cb3de318de147b0038dc93fbc71/black-24.1.1.tar.gz", hash = "sha256:48b5760dcbfe5cf97fd4fba23946681f3a81514c6ab8a45b50da67ac8fbc6c7b", size = 623755, upload-time = "2024-01-28T05:28:48.365Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/17/9e/104321dd49d30f7e9475afef76db7ad14b43f56933a315a657504d8fbdd7/black-24.1.1-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:e2c8dfa14677f90d976f68e0c923947ae68fa3961d61ee30976c388adc0b02c8", size = 1567927, upload-time = "2024-01-28T05:43:39.588Z" },
            { url = "https://files.pythonhosted.org/packages/be/ff/9380fb957347ab897543b53228cfd85112e421bdaf243e3865fa2d5e80ce/black-24.1.1-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:a21725862d0e855ae05da1dd25e3825ed712eaaccef6b03017fe0853a01aa45e", size = 1397655, upload-time = "2024-01-28T05:39:08.418Z" },
            { url = "https://files.pythonhosted.org/packages/55/14/07a41fb78fe81aa4852f16af4211fab5a130fcd3150b44a336042a3252d5/black-24.1.1-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:07204d078e25327aad9ed2c64790d681238686bce254c910de640c7cc4fc3aa6", size = 1718031, upload-time = "2024-01-28T05:31:22.398Z" },
            { url = "https://files.pythonhosted.org/packages/e5/fa/eaa2c165840a2496654366fcdc17f63459b89e3296b9269a18ba6d71f596/black-24.1.1-cp312-cp312-win_amd64.whl", hash = "sha256:a83fe522d9698d8f9a101b860b1ee154c1d25f8a82ceb807d319f085b2627c5b", size = 1350588, upload-time = "2024-01-28T05:32:22.839Z" },
            { url = "https://files.pythonhosted.org/packages/95/f3/c3d59ae490c627950efc97a27c3f73776577e2ec32d35737e72aee3d6738/black-24.1.1-py3-none-any.whl", hash = "sha256:5cdc2e2195212208fbcae579b931407c1fa9997584f0a415421748aeafff1168", size = 195702, upload-time = "2024-01-28T05:28:45.636Z" },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121, upload-time = "2023-08-17T17:29:11.868Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941, upload-time = "2023-08-17T17:29:10.08Z" },
        ]

        [[package]]
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433, upload-time = "2023-02-04T12:11:27.157Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695, upload-time = "2023-02-04T12:11:25.002Z" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055, upload-time = "2024-01-31T01:00:36.02Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717, upload-time = "2024-01-31T01:00:34.019Z" },
        ]

        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Install with `{package}@{latest}`. `black` should be reinstalled with a more recent version.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("black@latest")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     - black==24.1.1
     + black==24.3.0
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "black" }]

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
        sdist = { url = "https://files.pythonhosted.org/packages/8f/5f/bac24a952668c7482cfdb4ebf91ba57a796c9da8829363a772040c1a3312/black-24.3.0.tar.gz", hash = "sha256:a0c9c4a0771afc6919578cec71ce82a3e31e054904e7197deacbc9382671c41f", size = 634292, upload-time = "2024-03-15T19:35:43.699Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b6/c6/1d174efa9ff02b22d0124c73fc5f4d4fb006d0d9a081aadc354d05754a13/black-24.3.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:2818cf72dfd5d289e48f37ccfa08b460bf469e67fb7c4abb07edc2e9f16fb63f", size = 1600822, upload-time = "2024-03-15T19:45:20.337Z" },
            { url = "https://files.pythonhosted.org/packages/d9/ed/704731afffe460b8ff0672623b40fce9fe569f2ee617c15857e4d4440a3a/black-24.3.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:4acf672def7eb1725f41f38bf6bf425c8237248bb0804faa3965c036f7672d11", size = 1429987, upload-time = "2024-03-15T19:45:00.637Z" },
            { url = "https://files.pythonhosted.org/packages/a8/05/8dd038e30caadab7120176d4bc109b7ca2f4457f12eef746b0560a583458/black-24.3.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:c7ed6668cbbfcd231fa0dc1b137d3e40c04c7f786e626b405c62bcd5db5857e4", size = 1755319, upload-time = "2024-03-15T19:38:24.009Z" },
            { url = "https://files.pythonhosted.org/packages/71/9d/e5fa1ff4ef1940be15a64883c0bb8d2fcf626efec996eab4ae5a8c691d2c/black-24.3.0-cp312-cp312-win_amd64.whl", hash = "sha256:56f52cfbd3dabe2798d76dbdd299faa046a901041faf2cf33288bc4e6dae57b5", size = 1385180, upload-time = "2024-03-15T19:39:37.014Z" },
            { url = "https://files.pythonhosted.org/packages/4d/ea/31770a7e49f3eedfd8cd7b35e78b3a3aaad860400f8673994bc988318135/black-24.3.0-py3-none-any.whl", hash = "sha256:41622020d7120e01d377f74249e677039d20e6344ff5851de8a10f11f513bf93", size = 201493, upload-time = "2024-03-15T19:35:41.572Z" },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121, upload-time = "2023-08-17T17:29:11.868Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941, upload-time = "2023-08-17T17:29:10.08Z" },
        ]

        [[package]]
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433, upload-time = "2023-02-04T12:11:27.157Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695, upload-time = "2023-02-04T12:11:25.002Z" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055, upload-time = "2024-01-31T01:00:36.02Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717, upload-time = "2024-01-31T01:00:34.019Z" },
        ]

        [tool]
        requirements = [{ name = "black" }]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });
}

/// Install a tool with `--constraints`.
#[test]
fn tool_install_constraints() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let constraints_txt = context.temp_dir.child("constraints.txt");
    constraints_txt.write_str(indoc::indoc! {r"
        mypy-extensions<1
        anyio>=3
    "})?;

    // Install `black`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("black")
        .arg("--constraints")
        .arg(constraints_txt.as_os_str())
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==0.4.4
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "black" }]
        constraints = [
            { name = "anyio", specifier = ">=3" },
            { name = "mypy-extensions", specifier = "<1" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/8f/5f/bac24a952668c7482cfdb4ebf91ba57a796c9da8829363a772040c1a3312/black-24.3.0.tar.gz", hash = "sha256:a0c9c4a0771afc6919578cec71ce82a3e31e054904e7197deacbc9382671c41f", size = 634292, upload-time = "2024-03-15T19:35:43.699Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b6/c6/1d174efa9ff02b22d0124c73fc5f4d4fb006d0d9a081aadc354d05754a13/black-24.3.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:2818cf72dfd5d289e48f37ccfa08b460bf469e67fb7c4abb07edc2e9f16fb63f", size = 1600822, upload-time = "2024-03-15T19:45:20.337Z" },
            { url = "https://files.pythonhosted.org/packages/d9/ed/704731afffe460b8ff0672623b40fce9fe569f2ee617c15857e4d4440a3a/black-24.3.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:4acf672def7eb1725f41f38bf6bf425c8237248bb0804faa3965c036f7672d11", size = 1429987, upload-time = "2024-03-15T19:45:00.637Z" },
            { url = "https://files.pythonhosted.org/packages/a8/05/8dd038e30caadab7120176d4bc109b7ca2f4457f12eef746b0560a583458/black-24.3.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:c7ed6668cbbfcd231fa0dc1b137d3e40c04c7f786e626b405c62bcd5db5857e4", size = 1755319, upload-time = "2024-03-15T19:38:24.009Z" },
            { url = "https://files.pythonhosted.org/packages/71/9d/e5fa1ff4ef1940be15a64883c0bb8d2fcf626efec996eab4ae5a8c691d2c/black-24.3.0-cp312-cp312-win_amd64.whl", hash = "sha256:56f52cfbd3dabe2798d76dbdd299faa046a901041faf2cf33288bc4e6dae57b5", size = 1385180, upload-time = "2024-03-15T19:39:37.014Z" },
            { url = "https://files.pythonhosted.org/packages/4d/ea/31770a7e49f3eedfd8cd7b35e78b3a3aaad860400f8673994bc988318135/black-24.3.0-py3-none-any.whl", hash = "sha256:41622020d7120e01d377f74249e677039d20e6344ff5851de8a10f11f513bf93", size = 201493, upload-time = "2024-03-15T19:35:41.572Z" },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121, upload-time = "2023-08-17T17:29:11.868Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941, upload-time = "2023-08-17T17:29:10.08Z" },
        ]

        [[package]]
        name = "mypy-extensions"
        version = "0.4.4"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/d6/c6/7809c27b5c5dacb9f6537e9489d969b266f7091204c75a03048edcb4daf9/mypy_extensions-0.4.4.tar.gz", hash = "sha256:c8b707883a96efe9b4bb3aaf0dcc07e7e217d7d8368eec4db4049ee9e142f4fd", size = 4246, upload-time = "2023-02-04T11:55:11.417Z" }

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055, upload-time = "2024-01-31T01:00:36.02Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717, upload-time = "2024-01-31T01:00:34.019Z" },
        ]

        [tool]
        requirements = [{ name = "black" }]
        constraints = [
            { name = "mypy-extensions", specifier = "<1" },
            { name = "anyio", specifier = ">=3" },
        ]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    // Installing with the same constraints should be a no-op.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("black")
        .arg("--constraints")
        .arg(constraints_txt.as_os_str())
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    `black` is already installed
    ");

    let constraints_txt = context.temp_dir.child("constraints.txt");
    constraints_txt.write_str(indoc::indoc! {r"
        platformdirs<4
    "})?;

    // Installing with revised constraints should reinstall the tool.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("black")
        .arg("--constraints")
        .arg(constraints_txt.as_os_str())
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     - platformdirs==4.2.0
     + platformdirs==3.11.0
    Installed 2 executables: black, blackd
    ");

    Ok(())
}

/// Install a tool with `--overrides`.
#[test]
fn tool_install_overrides() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    let overrides_txt = context.temp_dir.child("overrides.txt");
    overrides_txt.write_str(indoc::indoc! {r"
        click<8
        anyio>=3
    "})?;

    // Install `black`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--overrides")
        .arg(overrides_txt.as_os_str())
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==7.1.2
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("black").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "black" }]
        overrides = [
            { name = "anyio", specifier = ">=3" },
            { name = "click", specifier = "<8" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/8f/5f/bac24a952668c7482cfdb4ebf91ba57a796c9da8829363a772040c1a3312/black-24.3.0.tar.gz", hash = "sha256:a0c9c4a0771afc6919578cec71ce82a3e31e054904e7197deacbc9382671c41f", size = 634292, upload-time = "2024-03-15T19:35:43.699Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b6/c6/1d174efa9ff02b22d0124c73fc5f4d4fb006d0d9a081aadc354d05754a13/black-24.3.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:2818cf72dfd5d289e48f37ccfa08b460bf469e67fb7c4abb07edc2e9f16fb63f", size = 1600822, upload-time = "2024-03-15T19:45:20.337Z" },
            { url = "https://files.pythonhosted.org/packages/d9/ed/704731afffe460b8ff0672623b40fce9fe569f2ee617c15857e4d4440a3a/black-24.3.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:4acf672def7eb1725f41f38bf6bf425c8237248bb0804faa3965c036f7672d11", size = 1429987, upload-time = "2024-03-15T19:45:00.637Z" },
            { url = "https://files.pythonhosted.org/packages/a8/05/8dd038e30caadab7120176d4bc109b7ca2f4457f12eef746b0560a583458/black-24.3.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:c7ed6668cbbfcd231fa0dc1b137d3e40c04c7f786e626b405c62bcd5db5857e4", size = 1755319, upload-time = "2024-03-15T19:38:24.009Z" },
            { url = "https://files.pythonhosted.org/packages/71/9d/e5fa1ff4ef1940be15a64883c0bb8d2fcf626efec996eab4ae5a8c691d2c/black-24.3.0-cp312-cp312-win_amd64.whl", hash = "sha256:56f52cfbd3dabe2798d76dbdd299faa046a901041faf2cf33288bc4e6dae57b5", size = 1385180, upload-time = "2024-03-15T19:39:37.014Z" },
            { url = "https://files.pythonhosted.org/packages/4d/ea/31770a7e49f3eedfd8cd7b35e78b3a3aaad860400f8673994bc988318135/black-24.3.0-py3-none-any.whl", hash = "sha256:41622020d7120e01d377f74249e677039d20e6344ff5851de8a10f11f513bf93", size = 201493, upload-time = "2024-03-15T19:35:41.572Z" },
        ]

        [[package]]
        name = "click"
        version = "7.1.2"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/27/6f/be940c8b1f1d69daceeb0032fee6c34d7bd70e3e649ccac0951500b4720e/click-7.1.2.tar.gz", hash = "sha256:d2b5255c7c6349bc1bd1e59e08cd12acbbd63ce649f2588755783aa94dfb6b1a", size = 297279, upload-time = "2020-04-27T20:22:45.014Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d2/3d/fa76db83bf75c4f8d338c2fd15c8d33fdd7ad23a9b5e57eb6c5de26b430e/click-7.1.2-py2.py3-none-any.whl", hash = "sha256:dacca89f4bfadd5de3d7489b7c8a566eee0d3676333fbb50030263894c38c0dc", size = 82780, upload-time = "2020-04-27T20:22:42.629Z" },
        ]

        [[package]]
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433, upload-time = "2023-02-04T12:11:27.157Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695, upload-time = "2023-02-04T12:11:25.002Z" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055, upload-time = "2024-01-31T01:00:36.02Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717, upload-time = "2024-01-31T01:00:34.019Z" },
        ]

        [tool]
        requirements = [{ name = "black" }]
        overrides = [
            { name = "click", specifier = "<8" },
            { name = "anyio", specifier = ">=3" },
        ]
        entrypoints = [
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    Ok(())
}

/// `uv tool install python` is not allowed
#[test]
fn tool_install_python() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `python`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot install Python with `uv tool install`. Did you mean to use `uv python install`?
    ");

    // Install `python@<version>`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("python@3.12")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot install Python with `uv tool install`. Did you mean to use `uv python install`?
    ");
}

#[test]
fn tool_install_mismatched_name() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--from")
        .arg("https://files.pythonhosted.org/packages/af/47/93213ee66ef8fae3b93b3e29206f6b251e65c97bd91d8e1c5596ef15af0a/flask-3.1.0-py3-none-any.whl")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Package name (`flask`) provided with `--from` does not match install request (`black`)
    ");

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--from")
        .arg("flask @ https://files.pythonhosted.org/packages/af/47/93213ee66ef8fae3b93b3e29206f6b251e65c97bd91d8e1c5596ef15af0a/flask-3.1.0-py3-none-any.whl")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Package name (`flask`) provided with `--from` does not match install request (`black`)
    ");

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("flask")
        .arg("--from")
        .arg("black @ https://files.pythonhosted.org/packages/af/47/93213ee66ef8fae3b93b3e29206f6b251e65c97bd91d8e1c5596ef15af0a/flask-3.1.0-py3-none-any.whl")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Package name (`black`) provided with `--from` does not match install request (`flask`)
    ");
}

/// When installing from an authenticated index, the credentials should be omitted from the receipt.
#[tokio::test]
async fn tool_install_credentials() {
    let proxy = crate::pypi_proxy::start().await;
    let context = uv_test::test_context!("3.12")
        .with_exclude_newer("2025-01-18T00:00:00Z")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `executable-application`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("executable-application")
         .arg("--index")
        .arg(proxy.authenticated_url("public", "heron", "/basic-auth/simple"))
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + executable-application==0.3.0
    Installed 1 executable: app
    ");

    tool_dir
        .child("executable-application")
        .assert(predicate::path::is_dir());
    tool_dir
        .child("executable-application")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("app{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run black in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r#"
        #![TEMP_DIR]/tools/executable-application/bin/python
        # -*- coding: utf-8 -*-
        import sys
        from executable_application import main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(main())
        "#);

    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("executable-application").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2025-01-18T00:00:00Z"

        [manifest]
        requirements = [{ name = "executable-application" }]

        [[package]]
        name = "executable-application"
        version = "0.3.0"
        source = { registry = "http://[LOCALHOST]/basic-auth/simple" }
        sdist = { url = "http://[LOCALHOST]/basic-auth/files/packages/9a/36/e803315469274d62f2dab543e3916c0b5b65730074d295f7d48711aa9e36/executable_application-0.3.0.tar.gz", hash = "sha256:0ef8c5ddd28649503c6e4a9f55be17e5b3bd0685df7b83ff7c260b481025f261", size = 914, upload-time = "2025-01-17T23:21:24.559Z" }
        wheels = [
            { url = "http://[LOCALHOST]/basic-auth/files/packages/32/97/8ab6fa1bbcb0a888f460c0a19c301f4cc4180573564ad7dd98b5ceca2ab6/executable_application-0.3.0-py3-none-any.whl", hash = "sha256:ca272aee7332e9d266663bc70037cd3ef1d74ffae40030eaf9ca46462dc8dcc6", size = 1719, upload-time = "2025-01-17T23:21:22.716Z" },
        ]

        [tool]
        requirements = [{ name = "executable-application" }]
        entrypoints = [
            { name = "app", install-path = "[TEMP_DIR]/bin/app", from = "executable-application" },
        ]

        [tool.options]
        index = [{ url = "http://[LOCALHOST]/basic-auth/simple", explicit = false, default = false, format = "simple", authenticate = "always" }]
        exclude-newer = "2025-01-18T00:00:00Z"
        "#);
    });
}

/// When installing from an authenticated index, the credentials should be omitted from the receipt.
#[tokio::test]
async fn tool_install_default_credentials() -> Result<()> {
    let proxy = crate::pypi_proxy::start().await;
    let context = uv_test::test_context!("3.12")
        .with_exclude_newer("2025-01-18T00:00:00Z")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Write a `uv.toml` with a default index that has credentials.
    let uv_toml = context.temp_dir.child("uv.toml");
    uv_toml.write_str(&format!(
        indoc::indoc! {r#"
        [[index]]
        url = "{}"
        default = true
        authenticate = "always"
    "#},
        proxy.authenticated_url("public", "heron", "/basic-auth/simple")
    ))?;

    // Install `executable-application`
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("executable-application")
        .arg("--config-file")
        .arg(uv_toml.as_os_str())
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + executable-application==0.3.0
    Installed 1 executable: app
    ");

    tool_dir
        .child("executable-application")
        .assert(predicate::path::is_dir());
    tool_dir
        .child("executable-application")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("app{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run black in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r#"
        #![TEMP_DIR]/tools/executable-application/bin/python
        # -*- coding: utf-8 -*-
        import sys
        from executable_application import main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(main())
        "#);
    });

    insta::with_settings!({
        filters => context.filters(),
    }, {
        // We should have a tool receipt
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("executable-application").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2025-01-18T00:00:00Z"

        [manifest]
        requirements = [{ name = "executable-application" }]

        [[package]]
        name = "executable-application"
        version = "0.3.0"
        source = { registry = "http://[LOCALHOST]/basic-auth/simple" }
        sdist = { url = "http://[LOCALHOST]/basic-auth/files/packages/9a/36/e803315469274d62f2dab543e3916c0b5b65730074d295f7d48711aa9e36/executable_application-0.3.0.tar.gz", hash = "sha256:0ef8c5ddd28649503c6e4a9f55be17e5b3bd0685df7b83ff7c260b481025f261", size = 914, upload-time = "2025-01-17T23:21:24.559Z" }
        wheels = [
            { url = "http://[LOCALHOST]/basic-auth/files/packages/32/97/8ab6fa1bbcb0a888f460c0a19c301f4cc4180573564ad7dd98b5ceca2ab6/executable_application-0.3.0-py3-none-any.whl", hash = "sha256:ca272aee7332e9d266663bc70037cd3ef1d74ffae40030eaf9ca46462dc8dcc6", size = 1719, upload-time = "2025-01-17T23:21:22.716Z" },
        ]

        [tool]
        requirements = [{ name = "executable-application" }]
        entrypoints = [
            { name = "app", install-path = "[TEMP_DIR]/bin/app", from = "executable-application" },
        ]

        [tool.options]
        index = [{ url = "http://[LOCALHOST]/basic-auth/simple", explicit = false, default = true, format = "simple", authenticate = "always" }]
        exclude-newer = "2025-01-18T00:00:00Z"
        "#);
    });

    // Attempt to upgrade without providing the credentials (from the config file).
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("executable-application")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    error: Failed to upgrade executable-application
      Caused by: Failed to fetch: `http://[LOCALHOST]/basic-auth/simple/executable-application/`
      Caused by: Missing credentials for http://[LOCALHOST]/basic-auth/simple/executable-application/
    ");

    // Attempt to upgrade.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .arg("executable-application")
        .arg("--config-file")
        .arg(uv_toml.as_os_str())
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Nothing to upgrade
    ");

    Ok(())
}

/// Test installing a tool with `--with-executables-from`.
#[test]
fn tool_install_with_executables_from() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--python-platform")
        .arg("linux")
        .arg("--with-executables-from")
        .arg("ansible-core,black")
        .arg("ansible==9.3.0")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + ansible==9.3.0
     + ansible-core==2.16.4
     + black==24.3.0
     + cffi==1.16.0
     + click==8.1.7
     + cryptography==42.0.5
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
     + pycparser==2.21
     + pyyaml==6.0.1
     + resolvelib==1.0.1
    Installed 11 executables from `ansible-core`: ansible, ansible-config, ansible-connection, ansible-console, ansible-doc, ansible-galaxy, ansible-inventory, ansible-playbook, ansible-pull, ansible-test, ansible-vault
    Installed 2 executables from `black`: black, blackd
    Installed 1 executable: ansible-community
    ");

    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(fs_err::read_to_string(tool_dir.join("ansible").join("uv-receipt.toml")).unwrap(), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [
            { name = "ansible", specifier = "==9.3.0" },
            { name = "ansible-core" },
            { name = "black" },
        ]

        [[package]]
        name = "ansible"
        version = "9.3.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "ansible-core" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/8a/de/a0a57da24b922bcc2340acbe6c9300e35a6fe132e3e6945333810732cc9c/ansible-9.3.0.tar.gz", hash = "sha256:7f4ea0e4d065538879b3e11e81e85eed4d802d1940f6564ad950e9d11a31b03c", size = 38240168, upload-time = "2024-02-27T18:17:53.53Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b9/39/7a6f7698f2af3393600653200543ba41e67edaa10b4efe729815031cc850/ansible-9.3.0-py3-none-any.whl", hash = "sha256:471993dd239611b4b6134e46911612f85639035f10d82b6c888528b5ffb3b16a", size = 46315787, upload-time = "2024-02-27T18:17:45.287Z" },
        ]

        [[package]]
        name = "ansible-core"
        version = "2.16.4"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "cryptography" },
            { name = "jinja2" },
            { name = "packaging" },
            { name = "pyyaml" },
            { name = "resolvelib" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/af/9c/12755b9ec6b696701fc7a33e8eab7a78f2b769dc0c966079c9005fffc7bf/ansible-core-2.16.4.tar.gz", hash = "sha256:2cd208b0915948c88bffad331e5d07097b6edca1872cb53375e51b6719e6a060", size = 3170397, upload-time = "2024-02-26T21:08:48.392Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/f8/bd/d7152b5b78e9363d30eb74b342c7dd577bf85ed28bcd8f4a8b02dce56553/ansible_core-2.16.4-py3-none-any.whl", hash = "sha256:c55d9a5f55651eb6c7f004ca9a9ed854d8cc310e6b438d96cea051cf3d2b2710", size = 2250452, upload-time = "2024-02-26T21:08:44.306Z" },
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
        sdist = { url = "https://files.pythonhosted.org/packages/8f/5f/bac24a952668c7482cfdb4ebf91ba57a796c9da8829363a772040c1a3312/black-24.3.0.tar.gz", hash = "sha256:a0c9c4a0771afc6919578cec71ce82a3e31e054904e7197deacbc9382671c41f", size = 634292, upload-time = "2024-03-15T19:35:43.699Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/b6/c6/1d174efa9ff02b22d0124c73fc5f4d4fb006d0d9a081aadc354d05754a13/black-24.3.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:2818cf72dfd5d289e48f37ccfa08b460bf469e67fb7c4abb07edc2e9f16fb63f", size = 1600822, upload-time = "2024-03-15T19:45:20.337Z" },
            { url = "https://files.pythonhosted.org/packages/d9/ed/704731afffe460b8ff0672623b40fce9fe569f2ee617c15857e4d4440a3a/black-24.3.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:4acf672def7eb1725f41f38bf6bf425c8237248bb0804faa3965c036f7672d11", size = 1429987, upload-time = "2024-03-15T19:45:00.637Z" },
            { url = "https://files.pythonhosted.org/packages/a8/05/8dd038e30caadab7120176d4bc109b7ca2f4457f12eef746b0560a583458/black-24.3.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:c7ed6668cbbfcd231fa0dc1b137d3e40c04c7f786e626b405c62bcd5db5857e4", size = 1755319, upload-time = "2024-03-15T19:38:24.009Z" },
            { url = "https://files.pythonhosted.org/packages/71/9d/e5fa1ff4ef1940be15a64883c0bb8d2fcf626efec996eab4ae5a8c691d2c/black-24.3.0-cp312-cp312-win_amd64.whl", hash = "sha256:56f52cfbd3dabe2798d76dbdd299faa046a901041faf2cf33288bc4e6dae57b5", size = 1385180, upload-time = "2024-03-15T19:39:37.014Z" },
            { url = "https://files.pythonhosted.org/packages/4d/ea/31770a7e49f3eedfd8cd7b35e78b3a3aaad860400f8673994bc988318135/black-24.3.0-py3-none-any.whl", hash = "sha256:41622020d7120e01d377f74249e677039d20e6344ff5851de8a10f11f513bf93", size = 201493, upload-time = "2024-03-15T19:35:41.572Z" },
        ]

        [[package]]
        name = "cffi"
        version = "1.16.0"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "pycparser" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/68/ce/95b0bae7968c65473e1298efb042e10cafc7bafc14d9e4f154008241c91d/cffi-1.16.0.tar.gz", hash = "sha256:bcb3ef43e58665bbda2fb198698fcae6776483e0c4a631aa5647806c25e02cc0", size = 512873, upload-time = "2023-09-28T18:02:04.656Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/22/04/1d10d5baf3faaae9b35f6c49bcf25c1be81ea68cc7ee6923206d02be85b0/cffi-1.16.0-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:fa3a0128b152627161ce47201262d3140edb5a5c3da88d73a1b790a959126956", size = 183322, upload-time = "2023-09-28T18:01:06.935Z" },
            { url = "https://files.pythonhosted.org/packages/b4/f6/b28d2bfb5fca9e8f9afc9d05eae245bed9f6ba5c2897fefee7a9abeaf091/cffi-1.16.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:68e7c44931cc171c54ccb702482e9fc723192e88d25a0e133edd7aff8fcd1f6e", size = 177173, upload-time = "2023-09-28T18:01:09.15Z" },
            { url = "https://files.pythonhosted.org/packages/9b/1a/575200306a3dfd9102ce573e7158d459a1bd7e44637e4f22a999c4fd64b1/cffi-1.16.0-cp312-cp312-manylinux_2_12_i686.manylinux2010_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:abd808f9c129ba2beda4cfc53bde801e5bcf9d6e0f22f095e45327c038bfe68e", size = 453846, upload-time = "2023-09-28T18:01:10.804Z" },
            { url = "https://files.pythonhosted.org/packages/e4/c7/c09cc6fd1828ea950e60d44e0ef5ed0b7e3396fbfb856e49ca7d629b1408/cffi-1.16.0-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:88e2b3c14bdb32e440be531ade29d3c50a1a59cd4e51b1dd8b0865c54ea5d2e2", size = 477041, upload-time = "2023-09-28T18:01:12.688Z" },
            { url = "https://files.pythonhosted.org/packages/b4/5f/c6e7e8d80fbf727909e4b1b5b9352082fc1604a14991b1d536bfaee5a36c/cffi-1.16.0-cp312-cp312-manylinux_2_17_ppc64le.manylinux2014_ppc64le.whl", hash = "sha256:fcc8eb6d5902bb1cf6dc4f187ee3ea80a1eba0a89aba40a5cb20a5087d961357", size = 483787, upload-time = "2023-09-28T18:01:14.974Z" },
            { url = "https://files.pythonhosted.org/packages/a3/81/5f5d61338951afa82ce4f0f777518708893b9420a8b309cc037fbf114e63/cffi-1.16.0-cp312-cp312-manylinux_2_17_s390x.manylinux2014_s390x.whl", hash = "sha256:b7be2d771cdba2942e13215c4e340bfd76398e9227ad10402a8767ab1865d2e6", size = 469137, upload-time = "2023-09-28T18:01:17.187Z" },
            { url = "https://files.pythonhosted.org/packages/09/d4/8759cc3b2222c159add8ce3af0089912203a31610f4be4c36f98e320b4c6/cffi-1.16.0-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:e715596e683d2ce000574bae5d07bd522c781a822866c20495e52520564f0969", size = 477578, upload-time = "2023-09-28T18:01:19.538Z" },
            { url = "https://files.pythonhosted.org/packages/4c/00/e17e2a8df0ff5aca2edd9eeebd93e095dd2515f2dd8d591d84a3233518f6/cffi-1.16.0-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:2d92b25dbf6cae33f65005baf472d2c245c050b1ce709cc4588cdcdd5495b520", size = 487099, upload-time = "2023-09-28T18:01:21.884Z" },
            { url = "https://files.pythonhosted.org/packages/c9/6e/751437067affe7ac0944b1ad4856ec11650da77f0dd8f305fae1117ef7bb/cffi-1.16.0-cp312-cp312-win32.whl", hash = "sha256:b2ca4e77f9f47c55c194982e10f058db063937845bb2b7a86c84a6cfe0aefa8b", size = 173564, upload-time = "2023-09-28T18:01:23.527Z" },
            { url = "https://files.pythonhosted.org/packages/e9/63/e285470a4880a4f36edabe4810057bd4b562c6ddcc165eacf9c3c7210b40/cffi-1.16.0-cp312-cp312-win_amd64.whl", hash = "sha256:68678abf380b42ce21a5f2abde8efee05c114c2fdb2e9eef2efdb0257fba1235", size = 181956, upload-time = "2023-09-28T18:01:24.971Z" },
        ]

        [[package]]
        name = "click"
        version = "8.1.7"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/d3/f04c7bfcf5c1862a2a5b845c6b2b360488cf47af55dfa79c98f6a6bf98b5/click-8.1.7.tar.gz", hash = "sha256:ca9853ad459e787e2192211578cc907e7594e294c7ccc834310722b41b9ca6de", size = 336121, upload-time = "2023-08-17T17:29:11.868Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/00/2e/d53fa4befbf2cfa713304affc7ca780ce4fc1fd8710527771b58311a3229/click-8.1.7-py3-none-any.whl", hash = "sha256:ae74fb96c20a0277a1d615f1e4d73c8414f5a98db8b799a7931d1582f3390c28", size = 97941, upload-time = "2023-08-17T17:29:10.08Z" },
        ]

        [[package]]
        name = "cryptography"
        version = "42.0.5"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "cffi", marker = "platform_python_implementation != 'PyPy'" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/13/9e/a55763a32d340d7b06d045753c186b690e7d88780cafce5f88cb931536be/cryptography-42.0.5.tar.gz", hash = "sha256:6fe07eec95dfd477eb9530aef5bead34fec819b3aaf6c5bd6d20565da607bfe1", size = 671025, upload-time = "2024-02-24T01:17:48.141Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d1/f1/fd98e6e79242d9aeaf6a5d49639a7e85f05741575af14d3f4a1d477f572e/cryptography-42.0.5-cp37-abi3-macosx_10_12_universal2.whl", hash = "sha256:a30596bae9403a342c978fb47d9b0ee277699fa53bbafad14706af51fe543d16", size = 5883181, upload-time = "2024-02-24T01:17:24.437Z" },
            { url = "https://files.pythonhosted.org/packages/d9/f9/27dda069a9f9bfda7c75305e222d904cc2445acf5eab5c696ade57d36f1b/cryptography-42.0.5-cp37-abi3-macosx_10_12_x86_64.whl", hash = "sha256:b7ffe927ee6531c78f81aa17e684e2ff617daeba7f189f911065b2ea2d526dec", size = 3106715, upload-time = "2024-02-24T01:16:46.129Z" },
            { url = "https://files.pythonhosted.org/packages/e2/59/61b2364f2a4d3668d933531bc30d012b9b2de1e534df4805678471287d57/cryptography-42.0.5-cp37-abi3-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:2424ff4c4ac7f6b8177b53c17ed5d8fa74ae5955656867f5a8affaca36a27abb", size = 4376731, upload-time = "2024-02-24T01:17:03.742Z" },
            { url = "https://files.pythonhosted.org/packages/fb/0b/14509319a1b49858425553d2fb3808579cfdfe98c1d71a3f046c1b4e0108/cryptography-42.0.5-cp37-abi3-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:329906dcc7b20ff3cad13c069a78124ed8247adcac44b10bea1130e36caae0b4", size = 4568288, upload-time = "2024-02-24T01:16:43.458Z" },
            { url = "https://files.pythonhosted.org/packages/8c/50/9185cca136596448d9cc595ae22a9bd4412ad35d812550c37c1390d54673/cryptography-42.0.5-cp37-abi3-manylinux_2_28_aarch64.whl", hash = "sha256:b03c2ae5d2f0fc05f9a2c0c997e1bc18c8229f392234e8a0194f202169ccd278", size = 4362222, upload-time = "2024-02-24T01:16:33.882Z" },
            { url = "https://files.pythonhosted.org/packages/64/f7/d3c83c79947cc6807e6acd3b2d9a1cbd312042777bc7eec50c869913df79/cryptography-42.0.5-cp37-abi3-manylinux_2_28_x86_64.whl", hash = "sha256:f8837fe1d6ac4a8052a9a8ddab256bc006242696f03368a4009be7ee3075cdb7", size = 4578380, upload-time = "2024-02-24T01:17:21.958Z" },
            { url = "https://files.pythonhosted.org/packages/e5/61/67e090a41c70ee526bd5121b1ccabab85c727574332d03326baaedea962d/cryptography-42.0.5-cp37-abi3-musllinux_1_1_aarch64.whl", hash = "sha256:0270572b8bd2c833c3981724b8ee9747b3ec96f699a9665470018594301439ee", size = 4475683, upload-time = "2024-02-24T01:16:11.726Z" },
            { url = "https://files.pythonhosted.org/packages/5b/3d/c3c21e3afaf43bacccc3ebf61d1a0d47cef6e2607dbba01662f6f9d8fc40/cryptography-42.0.5-cp37-abi3-musllinux_1_1_x86_64.whl", hash = "sha256:b8cac287fafc4ad485b8a9b67d0ee80c66bf3574f655d3b97ef2e1082360faf1", size = 4651973, upload-time = "2024-02-24T01:16:57.816Z" },
            { url = "https://files.pythonhosted.org/packages/d8/b1/127ecb373d02db85a7a7de5093d7ac7b7714b8907d631f0591e8f002998d/cryptography-42.0.5-cp37-abi3-musllinux_1_2_aarch64.whl", hash = "sha256:16a48c23a62a2f4a285699dba2e4ff2d1cff3115b9df052cdd976a18856d8e3d", size = 4448866, upload-time = "2024-02-24T01:17:28.106Z" },
            { url = "https://files.pythonhosted.org/packages/2c/9c/821ef6144daf80360cf6093520bf07eec7c793103ed4b1bf3fa17d2b55d8/cryptography-42.0.5-cp37-abi3-musllinux_1_2_x86_64.whl", hash = "sha256:2bce03af1ce5a5567ab89bd90d11e7bbdff56b8af3acbbec1faded8f44cb06da", size = 4652546, upload-time = "2024-02-24T01:16:37.085Z" },
            { url = "https://files.pythonhosted.org/packages/86/7f/1c6bb9ef3c4e5e2a438ab2b7ac85af52a9aa9a9a9a326b89e1b25659b598/cryptography-42.0.5-cp37-abi3-win32.whl", hash = "sha256:b6cd2203306b63e41acdf39aa93b86fb566049aeb6dc489b70e34bcd07adca74", size = 2431140, upload-time = "2024-02-24T01:16:48.931Z" },
            { url = "https://files.pythonhosted.org/packages/36/33/ed48350d38a6a151dd3cf1850a5966b86c5752212ddaaceb44e65bf412e5/cryptography-42.0.5-cp37-abi3-win_amd64.whl", hash = "sha256:98d8dc6d012b82287f2c3d26ce1d2dd130ec200c8679b6213b3c73c08b2b7940", size = 2890092, upload-time = "2024-02-24T01:17:35.583Z" },
            { url = "https://files.pythonhosted.org/packages/6d/4d/f7c14c7a49e35df829e04d451a57b843208be7442c8e087250c195775be1/cryptography-42.0.5-cp39-abi3-macosx_10_12_universal2.whl", hash = "sha256:5e6275c09d2badf57aea3afa80d975444f4be8d3bc58f7f80d2a484c6f9485c8", size = 5881247, upload-time = "2024-02-24T01:16:54.4Z" },
            { url = "https://files.pythonhosted.org/packages/50/26/248cd8b6809635ed412159791c0d3869d8ec9dfdc57d428d500a14d425b7/cryptography-42.0.5-cp39-abi3-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:e4985a790f921508f36f81831817cbc03b102d643b5fcb81cd33df3fa291a1a1", size = 4376966, upload-time = "2024-02-24T01:16:19.43Z" },
            { url = "https://files.pythonhosted.org/packages/d4/fa/057f9d7a5364c86ccb6a4bd4e5c58920dcb66532be0cc21da3f9c7617ec3/cryptography-42.0.5-cp39-abi3-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:7cde5f38e614f55e28d831754e8a3bacf9ace5d1566235e39d91b35502d6936e", size = 4567683, upload-time = "2024-02-24T01:16:14.285Z" },
            { url = "https://files.pythonhosted.org/packages/0e/1d/62a2324882c0db89f64358dadfb95cae024ee3ba9fde3d5fd4d2f58af9f5/cryptography-42.0.5-cp39-abi3-manylinux_2_28_aarch64.whl", hash = "sha256:7367d7b2eca6513681127ebad53b2582911d1736dc2ffc19f2c3ae49997496bc", size = 4363579, upload-time = "2024-02-24T01:17:38.512Z" },
            { url = "https://files.pythonhosted.org/packages/48/c8/c0962598c43d3cff2c9d6ac66d0c612bdfb1975be8d87b8889960cf8c81d/cryptography-42.0.5-cp39-abi3-manylinux_2_28_x86_64.whl", hash = "sha256:cd2030f6650c089aeb304cf093f3244d34745ce0cfcc39f20c6fbfe030102e2a", size = 4578653, upload-time = "2024-02-24T01:17:16.343Z" },
            { url = "https://files.pythonhosted.org/packages/69/f6/630eb71f246208103ffee754b8375b6b334eeedb28620b3ae57be815eeeb/cryptography-42.0.5-cp39-abi3-musllinux_1_1_aarch64.whl", hash = "sha256:a2913c5375154b6ef2e91c10b5720ea6e21007412f6437504ffea2109b5a33d7", size = 4476954, upload-time = "2024-02-24T01:16:07.717Z" },
            { url = "https://files.pythonhosted.org/packages/7d/bc/b6c691c960b5dcd54c5444e73af7f826e62af965ba59b6d7e9928b6489a2/cryptography-42.0.5-cp39-abi3-musllinux_1_1_x86_64.whl", hash = "sha256:c41fb5e6a5fe9ebcd58ca3abfeb51dffb5d83d6775405305bfa8715b76521922", size = 4650638, upload-time = "2024-02-24T01:16:16.919Z" },
            { url = "https://files.pythonhosted.org/packages/c2/40/c7cb9d6819b90640ffc3c4028b28f46edc525feaeaa0d98ea23e843d446d/cryptography-42.0.5-cp39-abi3-musllinux_1_2_aarch64.whl", hash = "sha256:3eaafe47ec0d0ffcc9349e1708be2aaea4c6dd4978d76bf6eb0cb2c13636c6fc", size = 4450500, upload-time = "2024-02-24T01:17:06.895Z" },
            { url = "https://files.pythonhosted.org/packages/ca/2e/9f2c49bd6a18d46c05ec098b040e7d4599c61f50ced40a39adfae3f68306/cryptography-42.0.5-cp39-abi3-musllinux_1_2_x86_64.whl", hash = "sha256:1b95b98b0d2af784078fa69f637135e3c317091b615cd0905f8b8a087e86fa30", size = 4651722, upload-time = "2024-02-24T01:16:31.501Z" },
            { url = "https://files.pythonhosted.org/packages/93/56/2d8d8903513185743bc6f763797fcba1718093190943735aa2ce8f3f0328/cryptography-42.0.5-cp39-abi3-win32.whl", hash = "sha256:1f71c10d1e88467126f0efd484bd44bca5e14c664ec2ede64c32f20875c0d413", size = 2431150, upload-time = "2024-02-24T01:17:33.35Z" },
            { url = "https://files.pythonhosted.org/packages/e3/14/13acd84f2a8303d9410ba2e24534a9d90c2817583636a91c4f314224768d/cryptography-42.0.5-cp39-abi3-win_amd64.whl", hash = "sha256:a011a644f6d7d03736214d38832e030d8268bcff4a41f728e6030325fea3e400", size = 2891129, upload-time = "2024-02-24T01:16:22.758Z" },
        ]

        [[package]]
        name = "jinja2"
        version = "3.1.3"
        source = { registry = "https://pypi.org/simple" }
        dependencies = [
            { name = "markupsafe" },
        ]
        sdist = { url = "https://files.pythonhosted.org/packages/b2/5e/3a21abf3cd467d7876045335e681d276ac32492febe6d98ad89562d1a7e1/Jinja2-3.1.3.tar.gz", hash = "sha256:ac8bd6544d4bb2c9792bf3a159e80bba8fda7f07e81bc3aed565432d5925ba90", size = 268261, upload-time = "2024-01-10T23:12:21.133Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/30/6d/6de6be2d02603ab56e72997708809e8a5b0fbfee080735109b40a3564843/Jinja2-3.1.3-py3-none-any.whl", hash = "sha256:7d6d50dd97d52cbc355597bd845fabfbac3f551e1f99619e39a35ce8c370b5fa", size = 133236, upload-time = "2024-01-10T23:12:19.504Z" },
        ]

        [[package]]
        name = "markupsafe"
        version = "2.1.5"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/87/5b/aae44c6655f3801e81aa3eef09dbbf012431987ba564d7231722f68df02d/MarkupSafe-2.1.5.tar.gz", hash = "sha256:d283d37a890ba4c1ae73ffadf8046435c76e7bc2247bbb63c00bd1a709c6544b", size = 19384, upload-time = "2024-02-02T16:31:22.863Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/53/bd/583bf3e4c8d6a321938c13f49d44024dbe5ed63e0a7ba127e454a66da974/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_universal2.whl", hash = "sha256:8dec4936e9c3100156f8a2dc89c4b88d5c435175ff03413b443469c7c8c5f4d1", size = 18215, upload-time = "2024-02-02T16:30:33.081Z" },
            { url = "https://files.pythonhosted.org/packages/48/d6/e7cd795fc710292c3af3a06d80868ce4b02bfbbf370b7cee11d282815a2a/MarkupSafe-2.1.5-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:3c6b973f22eb18a789b1460b4b91bf04ae3f0c4234a0a6aa6b0a92f6f7b951d4", size = 14069, upload-time = "2024-02-02T16:30:34.148Z" },
            { url = "https://files.pythonhosted.org/packages/51/b5/5d8ec796e2a08fc814a2c7d2584b55f889a55cf17dd1a90f2beb70744e5c/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:ac07bad82163452a6884fe8fa0963fb98c2346ba78d779ec06bd7a6262132aee", size = 29452, upload-time = "2024-02-02T16:30:35.149Z" },
            { url = "https://files.pythonhosted.org/packages/0a/0d/2454f072fae3b5a137c119abf15465d1771319dfe9e4acbb31722a0fff91/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:f5dfb42c4604dddc8e4305050aa6deb084540643ed5804d7455b5df8fe16f5e5", size = 28462, upload-time = "2024-02-02T16:30:36.166Z" },
            { url = "https://files.pythonhosted.org/packages/2d/75/fd6cb2e68780f72d47e6671840ca517bda5ef663d30ada7616b0462ad1e3/MarkupSafe-2.1.5-cp312-cp312-manylinux_2_5_i686.manylinux1_i686.manylinux_2_17_i686.manylinux2014_i686.whl", hash = "sha256:ea3d8a3d18833cf4304cd2fc9cbb1efe188ca9b5efef2bdac7adc20594a0e46b", size = 27869, upload-time = "2024-02-02T16:30:37.834Z" },
            { url = "https://files.pythonhosted.org/packages/b0/81/147c477391c2750e8fc7705829f7351cf1cd3be64406edcf900dc633feb2/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_aarch64.whl", hash = "sha256:d050b3361367a06d752db6ead6e7edeb0009be66bc3bae0ee9d97fb326badc2a", size = 33906, upload-time = "2024-02-02T16:30:39.366Z" },
            { url = "https://files.pythonhosted.org/packages/8b/ff/9a52b71839d7a256b563e85d11050e307121000dcebc97df120176b3ad93/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_i686.whl", hash = "sha256:bec0a414d016ac1a18862a519e54b2fd0fc8bbfd6890376898a6c0891dd82e9f", size = 32296, upload-time = "2024-02-02T16:30:40.413Z" },
            { url = "https://files.pythonhosted.org/packages/88/07/2dc76aa51b481eb96a4c3198894f38b480490e834479611a4053fbf08623/MarkupSafe-2.1.5-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:58c98fee265677f63a4385256a6d7683ab1832f3ddd1e66fe948d5880c21a169", size = 33038, upload-time = "2024-02-02T16:30:42.243Z" },
            { url = "https://files.pythonhosted.org/packages/96/0c/620c1fb3661858c0e37eb3cbffd8c6f732a67cd97296f725789679801b31/MarkupSafe-2.1.5-cp312-cp312-win32.whl", hash = "sha256:8590b4ae07a35970728874632fed7bd57b26b0102df2d2b233b6d9d82f6c62ad", size = 16572, upload-time = "2024-02-02T16:30:43.326Z" },
            { url = "https://files.pythonhosted.org/packages/3f/14/c3554d512d5f9100a95e737502f4a2323a1959f6d0d01e0d0997b35f7b10/MarkupSafe-2.1.5-cp312-cp312-win_amd64.whl", hash = "sha256:823b65d8706e32ad2df51ed89496147a42a2a6e01c13cfb6ffb8b1e92bc910bb", size = 17127, upload-time = "2024-02-02T16:30:44.418Z" },
        ]

        [[package]]
        name = "mypy-extensions"
        version = "1.0.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/98/a4/1ab47638b92648243faf97a5aeb6ea83059cc3624972ab6b8d2316078d3f/mypy_extensions-1.0.0.tar.gz", hash = "sha256:75dbf8955dc00442a438fc4d0666508a9a97b6bd41aa2f0ffe9d2f2725af0782", size = 4433, upload-time = "2023-02-04T12:11:27.157Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/2a/e2/5d3f6ada4297caebe1a2add3b126fe800c96f56dbe5d1988a2cbe0b267aa/mypy_extensions-1.0.0-py3-none-any.whl", hash = "sha256:4392f6c0eb8a5668a69e23d168ffa70f0be9ccfd32b5cc2d26a34ae5b844552d", size = 4695, upload-time = "2023-02-04T12:11:25.002Z" },
        ]

        [[package]]
        name = "packaging"
        version = "24.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ee/b5/b43a27ac7472e1818c4bafd44430e69605baefe1f34440593e0332ec8b4d/packaging-24.0.tar.gz", hash = "sha256:eb82c5e3e56209074766e6885bb04b8c38a0c015d0a30036ebe7ece34c9989e9", size = 147882, upload-time = "2024-03-10T09:39:28.33Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/49/df/1fceb2f8900f8639e278b056416d49134fb8d84c5942ffaa01ad34782422/packaging-24.0-py3-none-any.whl", hash = "sha256:2ddfb553fdf02fb784c234c7ba6ccc288296ceabec964ad2eae3777778130bc5", size = 53488, upload-time = "2024-03-10T09:39:25.947Z" },
        ]

        [[package]]
        name = "pathspec"
        version = "0.12.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ca/bc/f35b8446f4531a7cb215605d100cd88b7ac6f44ab3fc94870c120ab3adbf/pathspec-0.12.1.tar.gz", hash = "sha256:a482d51503a1ab33b1c67a6c3813a26953dbdc71c31dacaef9a838c4e29f5712", size = 51043, upload-time = "2023-12-10T22:30:45Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/cc/20/ff623b09d963f88bfde16306a54e12ee5ea43e9b597108672ff3a408aad6/pathspec-0.12.1-py3-none-any.whl", hash = "sha256:a0d503e138a4c123b27490a4f7beda6a01c6f288df0e4a8b79c7eb0dc7b4cc08", size = 31191, upload-time = "2023-12-10T22:30:43.14Z" },
        ]

        [[package]]
        name = "platformdirs"
        version = "4.2.0"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/96/dc/c1d911bf5bb0fdc58cc05010e9f3efe3b67970cef779ba7fbc3183b987a8/platformdirs-4.2.0.tar.gz", hash = "sha256:ef0cc731df711022c174543cb70a9b5bd22e5a9337c8624ef2c2ceb8ddad8768", size = 20055, upload-time = "2024-01-31T01:00:36.02Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/55/72/4898c44ee9ea6f43396fbc23d9bfaf3d06e01b83698bdf2e4c919deceb7c/platformdirs-4.2.0-py3-none-any.whl", hash = "sha256:0614df2a2f37e1a662acbd8e2b25b92ccf8632929bc6d43467e17fe89c75e068", size = 17717, upload-time = "2024-01-31T01:00:34.019Z" },
        ]

        [[package]]
        name = "pycparser"
        version = "2.21"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/5e/0b/95d387f5f4433cb0f53ff7ad859bd2c6051051cebbb564f139a999ab46de/pycparser-2.21.tar.gz", hash = "sha256:e644fdec12f7872f86c58ff790da456218b10f863970249516d60a5eaca77206", size = 170877, upload-time = "2021-11-06T12:48:46.095Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/62/d5/5f610ebe421e85889f2e55e33b7f9a6795bd982198517d912eb1c76e1a53/pycparser-2.21-py2.py3-none-any.whl", hash = "sha256:8ee45429555515e1f6b185e78100aea234072576aa43ab53aefcae078162fca9", size = 118697, upload-time = "2021-11-06T12:50:13.61Z" },
        ]

        [[package]]
        name = "pyyaml"
        version = "6.0.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/cd/e5/af35f7ea75cf72f2cd079c95ee16797de7cd71f29ea7c68ae5ce7be1eda0/PyYAML-6.0.1.tar.gz", hash = "sha256:bfdf460b1736c775f2ba9f6a92bca30bc2095067b8a9d77876d1fad6cc3b4a43", size = 125201, upload-time = "2023-07-18T00:00:23.308Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/bc/06/1b305bf6aa704343be85444c9d011f626c763abb40c0edc1cad13bfd7f86/PyYAML-6.0.1-cp312-cp312-macosx_10_9_x86_64.whl", hash = "sha256:855fb52b0dc35af121542a76b9a84f8d1cd886ea97c84703eaa6d88e37a2ad28", size = 178692, upload-time = "2023-08-28T18:43:24.924Z" },
            { url = "https://files.pythonhosted.org/packages/84/02/404de95ced348b73dd84f70e15a41843d817ff8c1744516bf78358f2ffd2/PyYAML-6.0.1-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:40df9b996c2b73138957fe23a16a4f0ba614f4c0efce1e9406a184b6d07fa3a9", size = 165622, upload-time = "2023-08-28T18:43:26.54Z" },
            { url = "https://files.pythonhosted.org/packages/c7/4c/4a2908632fc980da6d918b9de9c1d9d7d7e70b2672b1ad5166ed27841ef7/PyYAML-6.0.1-cp312-cp312-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:a08c6f0fe150303c1c6b71ebcd7213c2858041a7e01975da3a99aed1e7a378ef", size = 696937, upload-time = "2024-01-18T20:40:22.92Z" },
            { url = "https://files.pythonhosted.org/packages/b4/33/720548182ffa8344418126017aa1d4ab4aeec9a2275f04ce3f3573d8ace8/PyYAML-6.0.1-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:6c22bec3fbe2524cde73d7ada88f6566758a8f7227bfbf93a408a9d86bcc12a0", size = 724969, upload-time = "2023-08-28T18:43:28.56Z" },
            { url = "https://files.pythonhosted.org/packages/4f/78/77b40157b6cb5f2d3d31a3d9b2efd1ba3505371f76730d267e8b32cf4b7f/PyYAML-6.0.1-cp312-cp312-musllinux_1_1_x86_64.whl", hash = "sha256:8d4e9c88387b0f5c7d5f281e55304de64cf7f9c0021a3525bd3b1c542da3b0e4", size = 712604, upload-time = "2023-08-28T18:43:30.206Z" },
            { url = "https://files.pythonhosted.org/packages/2e/97/3e0e089ee85e840f4b15bfa00e4e63d84a3691ababbfea92d6f820ea6f21/PyYAML-6.0.1-cp312-cp312-win32.whl", hash = "sha256:d483d2cdf104e7c9fa60c544d92981f12ad66a457afae824d146093b8c294c54", size = 126098, upload-time = "2023-08-28T18:43:31.835Z" },
            { url = "https://files.pythonhosted.org/packages/2b/9f/fbade56564ad486809c27b322d0f7e6a89c01f6b4fe208402e90d4443a99/PyYAML-6.0.1-cp312-cp312-win_amd64.whl", hash = "sha256:0d3304d8c0adc42be59c5f8a4d9e3d7379e6955ad754aa9d6ab7a398b59dd1df", size = 138675, upload-time = "2023-08-28T18:43:33.613Z" },
        ]

        [[package]]
        name = "resolvelib"
        version = "1.0.1"
        source = { registry = "https://pypi.org/simple" }
        sdist = { url = "https://files.pythonhosted.org/packages/ce/10/f699366ce577423cbc3df3280063099054c23df70856465080798c6ebad6/resolvelib-1.0.1.tar.gz", hash = "sha256:04ce76cbd63fded2078ce224785da6ecd42b9564b1390793f64ddecbe997b309", size = 21065, upload-time = "2023-03-09T05:10:38.292Z" }
        wheels = [
            { url = "https://files.pythonhosted.org/packages/d2/fc/e9ccf0521607bcd244aa0b3fbd574f71b65e9ce6a112c83af988bbbe2e23/resolvelib-1.0.1-py2.py3-none-any.whl", hash = "sha256:d2da45d1a8dfee81bdd591647783e340ef3bcb104b54c383f70d422ef5cc7dbf", size = 17194, upload-time = "2023-03-09T05:10:36.214Z" },
        ]

        [tool]
        requirements = [
            { name = "ansible", specifier = "==9.3.0" },
            { name = "ansible-core" },
            { name = "black" },
        ]
        entrypoints = [
            { name = "ansible", install-path = "[TEMP_DIR]/bin/ansible", from = "ansible-core" },
            { name = "ansible-community", install-path = "[TEMP_DIR]/bin/ansible-community", from = "ansible" },
            { name = "ansible-config", install-path = "[TEMP_DIR]/bin/ansible-config", from = "ansible-core" },
            { name = "ansible-connection", install-path = "[TEMP_DIR]/bin/ansible-connection", from = "ansible-core" },
            { name = "ansible-console", install-path = "[TEMP_DIR]/bin/ansible-console", from = "ansible-core" },
            { name = "ansible-doc", install-path = "[TEMP_DIR]/bin/ansible-doc", from = "ansible-core" },
            { name = "ansible-galaxy", install-path = "[TEMP_DIR]/bin/ansible-galaxy", from = "ansible-core" },
            { name = "ansible-inventory", install-path = "[TEMP_DIR]/bin/ansible-inventory", from = "ansible-core" },
            { name = "ansible-playbook", install-path = "[TEMP_DIR]/bin/ansible-playbook", from = "ansible-core" },
            { name = "ansible-pull", install-path = "[TEMP_DIR]/bin/ansible-pull", from = "ansible-core" },
            { name = "ansible-test", install-path = "[TEMP_DIR]/bin/ansible-test", from = "ansible-core" },
            { name = "ansible-vault", install-path = "[TEMP_DIR]/bin/ansible-vault", from = "ansible-core" },
            { name = "black", install-path = "[TEMP_DIR]/bin/black", from = "black" },
            { name = "blackd", install-path = "[TEMP_DIR]/bin/blackd", from = "black" },
        ]

        [tool.options]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    uv_snapshot!(context.filters(), context.tool_uninstall()
        .arg("ansible")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 14 executables: ansible, ansible-community, ansible-config, ansible-connection, ansible-console, ansible-doc, ansible-galaxy, ansible-inventory, ansible-playbook, ansible-pull, ansible-test, ansible-vault, black, blackd
    ");
}

/// Test installing a tool with `--with-executables-from`, but the package has no entrypoints.
#[test]
fn tool_install_with_executables_from_no_entrypoints() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Try to install flask with executables from requests (which has no executables)
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--with-executables-from")
        .arg("requests")
        .arg("flask")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    No executables are provided by package `requests`
    hint: Use `--with requests` to include `requests` as a dependency without installing its executables.

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + blinker==1.7.0
     + certifi==2024.2.2
     + charset-normalizer==3.3.2
     + click==8.1.7
     + flask==3.0.2
     + idna==3.6
     + itsdangerous==2.1.2
     + jinja2==3.1.3
     + markupsafe==2.1.5
     + requests==2.31.0
     + urllib3==2.2.1
     + werkzeug==3.0.1
    Installed 1 executable: flask
    ");
}

#[test]
fn tool_install_find_links() {
    let context = uv_test::test_context!("3.13").with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Run with `--find-links`.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--find-links")
        .arg(context.workspace_root.join("test/links/"))
        .arg("basic-app")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello from basic-app!

    ----- stderr -----
    Resolved 1 package in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + basic-app==0.1.0
    ");

    // Install with `--find-links`.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("--find-links")
        .arg(context.workspace_root.join("test/links/"))
        .arg("basic-app")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 1 package in [TIME]
    Installed 1 package in [TIME]
     + basic-app==0.1.0
    Installed 1 executable: basic-app
    ");

    tool_dir
        .child("basic-app")
        .assert(predicate::path::is_dir());
    tool_dir
        .child("basic-app")
        .child("uv-receipt.toml")
        .assert(predicate::path::exists());

    let executable = bin_dir.child(format!("basic-app{}", std::env::consts::EXE_SUFFIX));
    assert!(executable.exists());

    // On Windows, we can't snapshot an executable file.
    #[cfg(not(windows))]
    insta::with_settings!({
        filters => context.filters(),
    }, {
        // Should run basic-app in the virtual environment
        assert_snapshot!(fs_err::read_to_string(executable).unwrap(), @r#"
        #![TEMP_DIR]/tools/basic-app/bin/python
        # -*- coding: utf-8 -*-
        import sys
        from basic_app import main
        if __name__ == "__main__":
            if sys.argv[0].endswith("-script.pyw"):
                sys.argv[0] = sys.argv[0][:-11]
            elif sys.argv[0].endswith(".exe"):
                sys.argv[0] = sys.argv[0][:-4]
            sys.exit(main())
        "#);
    });

    // Run the installed version with `--find-links` on the CLI again.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--offline")
        .arg("--find-links")
        .arg(context.workspace_root.join("test/links/"))
        .arg("basic-app")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Hello from basic-app!

    ----- stderr -----
    ");

    // Run the installed version without `--find-links`.
    uv_snapshot!(context.filters(), context.tool_run()
        .arg("--offline")
        .arg("basic-app")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str()), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
      × No solution found when resolving tool dependencies:
      ╰─▶ Because only basic-app==0.1 is available and basic-app==0.1 needs to be downloaded from a registry, we can conclude that all versions of basic-app cannot be used.
          And because you require basic-app, we can conclude that your requirements are unsatisfiable.

          hint: Packages were unavailable because the network was disabled. When the network is disabled, registry packages may only be read from the cache.
    ");
}

#[test]
fn tool_install_python_platform() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");

    // Install `black` for macos.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--python-platform")
        .arg("macos")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    // Install `black` for Linux.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--python-platform")
        .arg("linux")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     ~ black==24.3.0
    Installed 2 executables: black, blackd
    ");
}

/// Reinstalling a tool after the underlying Python has been removed.
///
/// Regression test for <https://github.com/astral-sh/uv/issues/16252>.
#[test]
fn tool_install_removed_python() {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix();
    let tool_dir = context.temp_dir.child("tools");
    let bin_dir = context.temp_dir.child("bin");
    let (_, python_executable) = context.python_versions.first().unwrap();

    // Install `black` with an explicit Python request.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--python")
        .arg(python_executable)
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");

    let tool_root = tool_dir.child("black");

    // Simulate the tool's interpreter disappearing without copying an arbitrary system prefix
    // like `/usr` into the test directory.
    #[cfg(unix)]
    {
        let tool_python = tool_root.child("bin").child("python");
        fs_err::remove_file(&tool_python).unwrap();
        fs_err::os::unix::fs::symlink(context.temp_dir.join("missing-python"), &tool_python)
            .unwrap();
    }

    #[cfg(windows)]
    {
        use uv_fs::Simplified;

        let pyvenv_cfg = tool_root.child("pyvenv.cfg");
        let broken_home = context.temp_dir.join("missing-python");
        let contents = fs_err::read_to_string(&pyvenv_cfg).unwrap();
        let contents = contents
            .lines()
            .map(|line| {
                if line.starts_with("home = ") {
                    format!("home = {}", broken_home.simplified_display())
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        fs_err::write(&pyvenv_cfg, format!("{contents}\n")).unwrap();
    }

    // Reinstalling should skip the broken Python install.
    uv_snapshot!(context.filters(), context.tool_install()
        .arg("black")
        .arg("--reinstall")
        .env(EnvVars::UV_TOOL_DIR, tool_dir.as_os_str())
        .env(EnvVars::XDG_BIN_HOME, bin_dir.as_os_str())
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + black==24.3.0
     + click==8.1.7
     + mypy-extensions==1.0.0
     + packaging==24.0
     + pathspec==0.12.1
     + platformdirs==4.2.0
    Installed 2 executables: black, blackd
    ");
}

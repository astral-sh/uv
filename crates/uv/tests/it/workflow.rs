use crate::common::{diff_snapshot, uv_snapshot, TestContext};
use anyhow::Result;
use assert_fs::fixture::{FileWriteStr, PathChild};
use insta::assert_snapshot;

#[test]
fn packse_add_remove_one_package() {
    let context = TestContext::new("3.12");
    context.copy_ecosystem_project("packse");

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 49 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(lock);
    });

    let diff = context.diff_lock(|context| {
        let mut add_cmd = context.add();
        add_cmd.arg("--no-sync").arg("tzdata");
        add_cmd
    });
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(diff, @r###"
        --- old
        +++ new
        @@ -307,20 +307,21 @@
         name = "packse"
         version = "0.0.0"
         source = { editable = "." }
         dependencies = [
             { name = "chevron-blue" },
             { name = "hatchling" },
             { name = "msgspec" },
             { name = "pyyaml" },
             { name = "setuptools" },
             { name = "twine" },
        +    { name = "tzdata" },
         ]

         [package.optional-dependencies]
         index = [
             { name = "pypiserver" },
         ]
         serve = [
             { name = "pypiserver" },
             { name = "watchfiles" },
         ]
        @@ -335,20 +336,21 @@
         [package.metadata]
         requires-dist = [
             { name = "chevron-blue", specifier = ">=0.2.1" },
             { name = "hatchling", specifier = ">=1.20.0" },
             { name = "msgspec", specifier = ">=0.18.4" },
             { name = "packse", extras = ["index"], marker = "extra == 'serve'" },
             { name = "pypiserver", marker = "extra == 'index'", specifier = ">=2.0.1" },
             { name = "pyyaml", specifier = ">=6.0.1" },
             { name = "setuptools", specifier = ">=69.1.1" },
             { name = "twine", specifier = ">=4.0.2" },
        +    { name = "tzdata", specifier = ">=2024.1" },
             { name = "watchfiles", marker = "extra == 'serve'", specifier = ">=0.21.0" },
         ]
         provides-extras = ["index", "serve"]

         [package.metadata.requires-dev]
         dev = [
             { name = "psutil", specifier = ">=5.9.7" },
             { name = "pytest", specifier = ">=7.4.3" },
             { name = "syrupy", specifier = ">=4.6.0" },
         ]
        @@ -601,20 +603,29 @@
             { name = "rfc3986" },
             { name = "rich" },
             { name = "urllib3" },
         ]
         sdist = { url = "https://files.pythonhosted.org/packages/d3/cc/8025ad5102a5c754023092143b8b511e184ec087dfbfb357d7d88fb82bff/twine-5.0.0.tar.gz", hash = "sha256:89b0cc7d370a4b66421cc6102f269aa910fe0f1861c124f573cf2ddedbc10cf4", size = 222119 }
         wheels = [
             { url = "https://files.pythonhosted.org/packages/9a/d4/4db90c4a2b8c1006ea3e6291f36b50b66e45887cf17b3b958b5d646fb837/twine-5.0.0-py3-none-any.whl", hash = "sha256:a262933de0b484c53408f9edae2e7821c1c45a3314ff2df9bdd343aa7ab8edc0", size = 37138 },
         ]

         [[package]]
        +name = "tzdata"
        +version = "2024.1"
        +source = { registry = "https://pypi.org/simple" }
        +sdist = { url = "https://files.pythonhosted.org/packages/74/5b/e025d02cb3b66b7b76093404392d4b44343c69101cc85f4d180dd5784717/tzdata-2024.1.tar.gz", hash = "sha256:2674120f8d891909751c38abcdfd386ac0a5a1127954fbc332af6b5ceae07efd", size = 190559 }
        +wheels = [
        +    { url = "https://files.pythonhosted.org/packages/65/58/f9c9e6be752e9fcb8b6a0ee9fb87e6e7a1f6bcab2cdc73f02bb7ba91ada0/tzdata-2024.1-py2.py3-none-any.whl", hash = "sha256:9068bc196136463f5245e51efda838afa15aaeca9903f49050dfa2679db4d252", size = 345370 },
        +]
        +
        +[[package]]
         name = "urllib3"
         version = "2.2.1"
         source = { registry = "https://pypi.org/simple" }
         sdist = { url = "https://files.pythonhosted.org/packages/7a/50/7fd50a27caa0652cd4caf224aa87741ea41d3265ad13f010886167cfcc79/urllib3-2.2.1.tar.gz", hash = "sha256:d0570876c61ab9e520d776c38acbbb5b05a776d3f9ff98a5c8fd5162a444cf19", size = 291020 }
         wheels = [
             { url = "https://files.pythonhosted.org/packages/a2/73/a68704750a7679d0b6d3ad7aa8d4da8e14e151ae82e6fee774e6e0d05ec8/urllib3-2.2.1-py3-none-any.whl", hash = "sha256:450b20ec296a467077128bff42b73080516e71b56ff59a60a02bef2232c4fa9d", size = 121067 },
         ]

         [[package]]
         name = "watchfiles"
        "###);
    });

    let diff = context.diff_lock(|context| {
        let mut remove_cmd = context.remove();
        remove_cmd.arg("--no-sync").arg("tzdata");
        remove_cmd
    });
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(diff, @r###"
        --- old
        +++ new
        @@ -307,21 +307,20 @@
         name = "packse"
         version = "0.0.0"
         source = { editable = "." }
         dependencies = [
             { name = "chevron-blue" },
             { name = "hatchling" },
             { name = "msgspec" },
             { name = "pyyaml" },
             { name = "setuptools" },
             { name = "twine" },
        -    { name = "tzdata" },
         ]

         [package.optional-dependencies]
         index = [
             { name = "pypiserver" },
         ]
         serve = [
             { name = "pypiserver" },
             { name = "watchfiles" },
         ]
        @@ -336,21 +335,20 @@
         [package.metadata]
         requires-dist = [
             { name = "chevron-blue", specifier = ">=0.2.1" },
             { name = "hatchling", specifier = ">=1.20.0" },
             { name = "msgspec", specifier = ">=0.18.4" },
             { name = "packse", extras = ["index"], marker = "extra == 'serve'" },
             { name = "pypiserver", marker = "extra == 'index'", specifier = ">=2.0.1" },
             { name = "pyyaml", specifier = ">=6.0.1" },
             { name = "setuptools", specifier = ">=69.1.1" },
             { name = "twine", specifier = ">=4.0.2" },
        -    { name = "tzdata", specifier = ">=2024.1" },
             { name = "watchfiles", marker = "extra == 'serve'", specifier = ">=0.21.0" },
         ]
         provides-extras = ["index", "serve"]

         [package.metadata.requires-dev]
         dev = [
             { name = "psutil", specifier = ">=5.9.7" },
             { name = "pytest", specifier = ">=7.4.3" },
             { name = "syrupy", specifier = ">=4.6.0" },
         ]
        @@ -600,29 +598,20 @@
             { name = "readme-renderer" },
             { name = "requests" },
             { name = "requests-toolbelt" },
             { name = "rfc3986" },
             { name = "rich" },
             { name = "urllib3" },
         ]
         sdist = { url = "https://files.pythonhosted.org/packages/d3/cc/8025ad5102a5c754023092143b8b511e184ec087dfbfb357d7d88fb82bff/twine-5.0.0.tar.gz", hash = "sha256:89b0cc7d370a4b66421cc6102f269aa910fe0f1861c124f573cf2ddedbc10cf4", size = 222119 }
         wheels = [
             { url = "https://files.pythonhosted.org/packages/9a/d4/4db90c4a2b8c1006ea3e6291f36b50b66e45887cf17b3b958b5d646fb837/twine-5.0.0-py3-none-any.whl", hash = "sha256:a262933de0b484c53408f9edae2e7821c1c45a3314ff2df9bdd343aa7ab8edc0", size = 37138 },
        -]
        -
        -[[package]]
        -name = "tzdata"
        -version = "2024.1"
        -source = { registry = "https://pypi.org/simple" }
        -sdist = { url = "https://files.pythonhosted.org/packages/74/5b/e025d02cb3b66b7b76093404392d4b44343c69101cc85f4d180dd5784717/tzdata-2024.1.tar.gz", hash = "sha256:2674120f8d891909751c38abcdfd386ac0a5a1127954fbc332af6b5ceae07efd", size = 190559 }
        -wheels = [
        -    { url = "https://files.pythonhosted.org/packages/65/58/f9c9e6be752e9fcb8b6a0ee9fb87e6e7a1f6bcab2cdc73f02bb7ba91ada0/tzdata-2024.1-py2.py3-none-any.whl", hash = "sha256:9068bc196136463f5245e51efda838afa15aaeca9903f49050dfa2679db4d252", size = 345370 },
         ]

         [[package]]
         name = "urllib3"
         version = "2.2.1"
         source = { registry = "https://pypi.org/simple" }
         sdist = { url = "https://files.pythonhosted.org/packages/7a/50/7fd50a27caa0652cd4caf224aa87741ea41d3265ad13f010886167cfcc79/urllib3-2.2.1.tar.gz", hash = "sha256:d0570876c61ab9e520d776c38acbbb5b05a776d3f9ff98a5c8fd5162a444cf19", size = 291020 }
         wheels = [
             { url = "https://files.pythonhosted.org/packages/a2/73/a68704750a7679d0b6d3ad7aa8d4da8e14e151ae82e6fee774e6e0d05ec8/urllib3-2.2.1-py3-none-any.whl", hash = "sha256:450b20ec296a467077128bff42b73080516e71b56ff59a60a02bef2232c4fa9d", size = 121067 },
         ]
        "###);
    });

    // Back to where we started.
    let new_lock = context.read("uv.lock");
    let diff = diff_snapshot(&lock, &new_lock);
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(diff, @r###""###);
    });
}

#[test]
fn packse_add_remove_existing_package_noop() {
    let context = TestContext::new("3.12");
    context.copy_ecosystem_project("packse");

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 49 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(lock);
    });

    let diff = context.diff_lock(|context| {
        let mut add_cmd = context.add();
        add_cmd.arg("--no-sync").arg("pyyaml");
        add_cmd
    });
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(diff, @"");
    });
}

/// This test adds a new direct dependency that was already a
/// transitive dependency.
#[test]
fn packse_promote_transitive_to_direct_then_remove() {
    let context = TestContext::new("3.12");
    context.copy_ecosystem_project("packse");

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 49 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(lock);
    });

    let diff = context.diff_lock(|context| {
        let mut add_cmd = context.add();
        add_cmd.arg("--no-sync").arg("sniffio");
        add_cmd
    });
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(diff, @r###"
        --- old
        +++ new
        @@ -306,20 +306,21 @@
         [[package]]
         name = "packse"
         version = "0.0.0"
         source = { editable = "." }
         dependencies = [
             { name = "chevron-blue" },
             { name = "hatchling" },
             { name = "msgspec" },
             { name = "pyyaml" },
             { name = "setuptools" },
        +    { name = "sniffio" },
             { name = "twine" },
         ]

         [package.optional-dependencies]
         index = [
             { name = "pypiserver" },
         ]
         serve = [
             { name = "pypiserver" },
             { name = "watchfiles" },
        @@ -334,20 +335,21 @@

         [package.metadata]
         requires-dist = [
             { name = "chevron-blue", specifier = ">=0.2.1" },
             { name = "hatchling", specifier = ">=1.20.0" },
             { name = "msgspec", specifier = ">=0.18.4" },
             { name = "packse", extras = ["index"], marker = "extra == 'serve'" },
             { name = "pypiserver", marker = "extra == 'index'", specifier = ">=2.0.1" },
             { name = "pyyaml", specifier = ">=6.0.1" },
             { name = "setuptools", specifier = ">=69.1.1" },
        +    { name = "sniffio", specifier = ">=1.3.1" },
             { name = "twine", specifier = ">=4.0.2" },
             { name = "watchfiles", marker = "extra == 'serve'", specifier = ">=0.21.0" },
         ]
         provides-extras = ["index", "serve"]

         [package.metadata.requires-dev]
         dev = [
             { name = "psutil", specifier = ">=5.9.7" },
             { name = "pytest", specifier = ">=7.4.3" },
             { name = "syrupy", specifier = ">=4.6.0" },
        "###);
    });

    let diff = context.diff_lock(|context| {
        let mut remove_cmd = context.remove();
        remove_cmd.arg("--no-sync").arg("sniffio");
        remove_cmd
    });
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(diff, @r###"
        --- old
        +++ new
        @@ -306,21 +306,20 @@
         [[package]]
         name = "packse"
         version = "0.0.0"
         source = { editable = "." }
         dependencies = [
             { name = "chevron-blue" },
             { name = "hatchling" },
             { name = "msgspec" },
             { name = "pyyaml" },
             { name = "setuptools" },
        -    { name = "sniffio" },
             { name = "twine" },
         ]

         [package.optional-dependencies]
         index = [
             { name = "pypiserver" },
         ]
         serve = [
             { name = "pypiserver" },
             { name = "watchfiles" },
        @@ -335,21 +334,20 @@

         [package.metadata]
         requires-dist = [
             { name = "chevron-blue", specifier = ">=0.2.1" },
             { name = "hatchling", specifier = ">=1.20.0" },
             { name = "msgspec", specifier = ">=0.18.4" },
             { name = "packse", extras = ["index"], marker = "extra == 'serve'" },
             { name = "pypiserver", marker = "extra == 'index'", specifier = ">=2.0.1" },
             { name = "pyyaml", specifier = ">=6.0.1" },
             { name = "setuptools", specifier = ">=69.1.1" },
        -    { name = "sniffio", specifier = ">=1.3.1" },
             { name = "twine", specifier = ">=4.0.2" },
             { name = "watchfiles", marker = "extra == 'serve'", specifier = ">=0.21.0" },
         ]
         provides-extras = ["index", "serve"]

         [package.metadata.requires-dev]
         dev = [
             { name = "psutil", specifier = ">=5.9.7" },
             { name = "pytest", specifier = ">=7.4.3" },
             { name = "syrupy", specifier = ">=4.6.0" },
        "###);
    });

    // Back to where we started.
    let new_lock = context.read("uv.lock");
    let diff = diff_snapshot(&lock, &new_lock);
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(diff, @r###""###);
    });
}

#[test]
fn jax_instability() -> Result<()> {
    let context = TestContext::new("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(
        r#"
        [project]
        name = "uv-lock-instability"
        version = "0.1.0"
        description = "whatever"
        requires-python = ">=3.9.0"
        dependencies = ["jax==0.4.17"]
        "#,
    )?;

    uv_snapshot!(context.filters(), context.lock(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 8 packages in [TIME]
    "###);

    let lock = context.read("uv.lock");
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(lock);
    });

    let diff = context.diff_lock(|context| {
        let mut add_cmd = context.add();
        add_cmd.arg("--no-sync").arg("tzdata");
        add_cmd
    });
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(diff, @r###"
        --- old
        +++ new
        @@ -9,21 +9,21 @@
         ]

         [options]
         exclude-newer = "2024-03-25T00:00:00Z"

         [[package]]
         name = "importlib-metadata"
         version = "7.1.0"
         source = { registry = "https://pypi.org/simple" }
         dependencies = [
        -    { name = "zipp" },
        +    { name = "zipp", marker = "python_full_version < '3.10'" },
         ]
         sdist = { url = "https://files.pythonhosted.org/packages/a0/fc/c4e6078d21fc4fa56300a241b87eae76766aa380a23fc450fc85bb7bf547/importlib_metadata-7.1.0.tar.gz", hash = "sha256:b78938b926ee8d5f020fc4772d487045805a55ddbad2ecf21c6d60938dc7fcd2", size = 52120 }
         wheels = [
             { url = "https://files.pythonhosted.org/packages/2d/0a/679461c511447ffaf176567d5c496d1de27cbe34a87df6677d7171b2fbd4/importlib_metadata-7.1.0-py3-none-any.whl", hash = "sha256:30962b96c0c223483ed6cc7280e7f0199feb01a0e40cfae4d4450fc6fab1f570", size = 24409 },
         ]

         [[package]]
         name = "jax"
         version = "0.4.17"
         source = { registry = "https://pypi.org/simple" }
        @@ -150,28 +150,41 @@
             { url = "https://files.pythonhosted.org/packages/f3/31/91a2a3c5eb85d2bfa86d7c98f2df5d77dcdefb3d80ca9f9037ad04393acf/scipy-1.12.0-cp312-cp312-win_amd64.whl", hash = "sha256:e646d8571804a304e1da01040d21577685ce8e2db08ac58e543eaca063453e1c", size = 45816713 },
             { url = "https://files.pythonhosted.org/packages/ed/be/49a3f999dc91f1a653847f38c34763dcdeaa8a327f3665bdfe9bf5555109/scipy-1.12.0-cp39-cp39-macosx_10_9_x86_64.whl", hash = "sha256:913d6e7956c3a671de3b05ccb66b11bc293f56bfdef040583a7221d9e22a2e35", size = 38929252 },
             { url = "https://files.pythonhosted.org/packages/32/48/f605bad3e610efe05a51b56698578f7a98f900513a4bad2c9f12df845cd6/scipy-1.12.0-cp39-cp39-macosx_12_0_arm64.whl", hash = "sha256:bba1b0c7256ad75401c73e4b3cf09d1f176e9bd4248f0d3112170fb2ec4db067", size = 31356374 },
             { url = "https://files.pythonhosted.org/packages/5f/40/ac3cc2719c67c97a88d746e93fda89b9447b65a47e408fdd415c370bab2a/scipy-1.12.0-cp39-cp39-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:730badef9b827b368f351eacae2e82da414e13cf8bd5051b4bdfd720271a5371", size = 34787482 },
             { url = "https://files.pythonhosted.org/packages/a6/9d/f864266894b67cdb5731ab531afba68713da3d6d8252f698ccab775d3f68/scipy-1.12.0-cp39-cp39-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:6546dc2c11a9df6926afcbdd8a3edec28566e4e785b915e849348c6dd9f3f490", size = 38473470 },
             { url = "https://files.pythonhosted.org/packages/43/e7/a170210e15434befff4dad019aa301a5c350f573b925a68dd84a57d86b43/scipy-1.12.0-cp39-cp39-musllinux_1_1_x86_64.whl", hash = "sha256:196ebad3a4882081f62a5bf4aeb7326aa34b110e533aab23e4374fcccb0890dc", size = 38659602 },
             { url = "https://files.pythonhosted.org/packages/92/f6/eb15f6086c82e62d98ae9f8644c518003e34c03b2ac25683ea932bb30047/scipy-1.12.0-cp39-cp39-win_amd64.whl", hash = "sha256:b360f1b6b2f742781299514e99ff560d1fe9bd1bff2712894b52abe528d1fd1e", size = 46211895 },
         ]

         [[package]]
        +name = "tzdata"
        +version = "2024.1"
        +source = { registry = "https://pypi.org/simple" }
        +sdist = { url = "https://files.pythonhosted.org/packages/74/5b/e025d02cb3b66b7b76093404392d4b44343c69101cc85f4d180dd5784717/tzdata-2024.1.tar.gz", hash = "sha256:2674120f8d891909751c38abcdfd386ac0a5a1127954fbc332af6b5ceae07efd", size = 190559 }
        +wheels = [
        +    { url = "https://files.pythonhosted.org/packages/65/58/f9c9e6be752e9fcb8b6a0ee9fb87e6e7a1f6bcab2cdc73f02bb7ba91ada0/tzdata-2024.1-py2.py3-none-any.whl", hash = "sha256:9068bc196136463f5245e51efda838afa15aaeca9903f49050dfa2679db4d252", size = 345370 },
        +]
        +
        +[[package]]
         name = "uv-lock-instability"
         version = "0.1.0"
         source = { virtual = "." }
         dependencies = [
             { name = "jax" },
        +    { name = "tzdata" },
         ]

         [package.metadata]
        -requires-dist = [{ name = "jax", specifier = "==0.4.17" }]
        +requires-dist = [
        +    { name = "jax", specifier = "==0.4.17" },
        +    { name = "tzdata", specifier = ">=2024.1" },
        +]

         [[package]]
         name = "zipp"
         version = "3.18.1"
         source = { registry = "https://pypi.org/simple" }
         sdist = { url = "https://files.pythonhosted.org/packages/3e/ef/65da662da6f9991e87f058bc90b91a935ae655a16ae5514660d6460d1298/zipp-3.18.1.tar.gz", hash = "sha256:2884ed22e7d8961de1c9a05142eb69a247f120291bc0206a00a7642f09b5b715", size = 21220 }
         wheels = [
             { url = "https://files.pythonhosted.org/packages/c2/0a/ba9d0ee9536d3ef73a3448e931776e658b36f128d344e175bc32b092a8bf/zipp-3.18.1-py3-none-any.whl", hash = "sha256:206f5a15f2af3dbaee80769fb7dc6f249695e940acca08dfb2a4769fe61e538b", size = 8247 },
         ]
        "###);
    });

    let diff = context.diff_lock(|context| {
        let mut remove_cmd = context.remove();
        remove_cmd.arg("--no-sync").arg("tzdata");
        remove_cmd
    });
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(diff, @r###"
        --- old
        +++ new
        @@ -150,41 +150,28 @@
             { url = "https://files.pythonhosted.org/packages/f3/31/91a2a3c5eb85d2bfa86d7c98f2df5d77dcdefb3d80ca9f9037ad04393acf/scipy-1.12.0-cp312-cp312-win_amd64.whl", hash = "sha256:e646d8571804a304e1da01040d21577685ce8e2db08ac58e543eaca063453e1c", size = 45816713 },
             { url = "https://files.pythonhosted.org/packages/ed/be/49a3f999dc91f1a653847f38c34763dcdeaa8a327f3665bdfe9bf5555109/scipy-1.12.0-cp39-cp39-macosx_10_9_x86_64.whl", hash = "sha256:913d6e7956c3a671de3b05ccb66b11bc293f56bfdef040583a7221d9e22a2e35", size = 38929252 },
             { url = "https://files.pythonhosted.org/packages/32/48/f605bad3e610efe05a51b56698578f7a98f900513a4bad2c9f12df845cd6/scipy-1.12.0-cp39-cp39-macosx_12_0_arm64.whl", hash = "sha256:bba1b0c7256ad75401c73e4b3cf09d1f176e9bd4248f0d3112170fb2ec4db067", size = 31356374 },
             { url = "https://files.pythonhosted.org/packages/5f/40/ac3cc2719c67c97a88d746e93fda89b9447b65a47e408fdd415c370bab2a/scipy-1.12.0-cp39-cp39-manylinux_2_17_aarch64.manylinux2014_aarch64.whl", hash = "sha256:730badef9b827b368f351eacae2e82da414e13cf8bd5051b4bdfd720271a5371", size = 34787482 },
             { url = "https://files.pythonhosted.org/packages/a6/9d/f864266894b67cdb5731ab531afba68713da3d6d8252f698ccab775d3f68/scipy-1.12.0-cp39-cp39-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", hash = "sha256:6546dc2c11a9df6926afcbdd8a3edec28566e4e785b915e849348c6dd9f3f490", size = 38473470 },
             { url = "https://files.pythonhosted.org/packages/43/e7/a170210e15434befff4dad019aa301a5c350f573b925a68dd84a57d86b43/scipy-1.12.0-cp39-cp39-musllinux_1_1_x86_64.whl", hash = "sha256:196ebad3a4882081f62a5bf4aeb7326aa34b110e533aab23e4374fcccb0890dc", size = 38659602 },
             { url = "https://files.pythonhosted.org/packages/92/f6/eb15f6086c82e62d98ae9f8644c518003e34c03b2ac25683ea932bb30047/scipy-1.12.0-cp39-cp39-win_amd64.whl", hash = "sha256:b360f1b6b2f742781299514e99ff560d1fe9bd1bff2712894b52abe528d1fd1e", size = 46211895 },
         ]

         [[package]]
        -name = "tzdata"
        -version = "2024.1"
        -source = { registry = "https://pypi.org/simple" }
        -sdist = { url = "https://files.pythonhosted.org/packages/74/5b/e025d02cb3b66b7b76093404392d4b44343c69101cc85f4d180dd5784717/tzdata-2024.1.tar.gz", hash = "sha256:2674120f8d891909751c38abcdfd386ac0a5a1127954fbc332af6b5ceae07efd", size = 190559 }
        -wheels = [
        -    { url = "https://files.pythonhosted.org/packages/65/58/f9c9e6be752e9fcb8b6a0ee9fb87e6e7a1f6bcab2cdc73f02bb7ba91ada0/tzdata-2024.1-py2.py3-none-any.whl", hash = "sha256:9068bc196136463f5245e51efda838afa15aaeca9903f49050dfa2679db4d252", size = 345370 },
        -]
        -
        -[[package]]
         name = "uv-lock-instability"
         version = "0.1.0"
         source = { virtual = "." }
         dependencies = [
             { name = "jax" },
        -    { name = "tzdata" },
         ]

         [package.metadata]
        -requires-dist = [
        -    { name = "jax", specifier = "==0.4.17" },
        -    { name = "tzdata", specifier = ">=2024.1" },
        -]
        +requires-dist = [{ name = "jax", specifier = "==0.4.17" }]

         [[package]]
         name = "zipp"
         version = "3.18.1"
         source = { registry = "https://pypi.org/simple" }
         sdist = { url = "https://files.pythonhosted.org/packages/3e/ef/65da662da6f9991e87f058bc90b91a935ae655a16ae5514660d6460d1298/zipp-3.18.1.tar.gz", hash = "sha256:2884ed22e7d8961de1c9a05142eb69a247f120291bc0206a00a7642f09b5b715", size = 21220 }
         wheels = [
             { url = "https://files.pythonhosted.org/packages/c2/0a/ba9d0ee9536d3ef73a3448e931776e658b36f128d344e175bc32b092a8bf/zipp-3.18.1-py3-none-any.whl", hash = "sha256:206f5a15f2af3dbaee80769fb7dc6f249695e940acca08dfb2a4769fe61e538b", size = 8247 },
         ]
        "###);
    });

    // Back to where we started.
    //
    // Note that this is wrong! This demonstrates that `uv` sometimes does
    // not produce a stable resolution.
    //
    // See: https://github.com/astral-sh/uv/issues/6063
    // See: https://github.com/astral-sh/uv/issues/6158
    let new_lock = context.read("uv.lock");
    let diff = diff_snapshot(&lock, &new_lock);
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(diff, @r###"
        --- old
        +++ new
        @@ -9,21 +9,21 @@
         ]

         [options]
         exclude-newer = "2024-03-25T00:00:00Z"

         [[package]]
         name = "importlib-metadata"
         version = "7.1.0"
         source = { registry = "https://pypi.org/simple" }
         dependencies = [
        -    { name = "zipp" },
        +    { name = "zipp", marker = "python_full_version < '3.10'" },
         ]
         sdist = { url = "https://files.pythonhosted.org/packages/a0/fc/c4e6078d21fc4fa56300a241b87eae76766aa380a23fc450fc85bb7bf547/importlib_metadata-7.1.0.tar.gz", hash = "sha256:b78938b926ee8d5f020fc4772d487045805a55ddbad2ecf21c6d60938dc7fcd2", size = 52120 }
         wheels = [
             { url = "https://files.pythonhosted.org/packages/2d/0a/679461c511447ffaf176567d5c496d1de27cbe34a87df6677d7171b2fbd4/importlib_metadata-7.1.0-py3-none-any.whl", hash = "sha256:30962b96c0c223483ed6cc7280e7f0199feb01a0e40cfae4d4450fc6fab1f570", size = 24409 },
         ]

         [[package]]
         name = "jax"
         version = "0.4.17"
         source = { registry = "https://pypi.org/simple" }
        "###);
    });

    Ok(())
}

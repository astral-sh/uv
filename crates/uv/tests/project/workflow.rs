use insta::assert_snapshot;
use uv_test::{diff_snapshot, uv_snapshot};

#[test]
fn packse_add_remove_one_package() {
    let context = uv_test::test_context!("3.12");
    context.copy_ecosystem_project("packse");

    uv_snapshot!(context.filters(), context.lock(), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The `tool.uv.dev-dependencies` field (used in `pyproject.toml`) is deprecated and will be removed in a future release; use `dependency-groups.dev` instead
    Resolved 49 packages in [TIME]
    ");

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
        assert_snapshot!(diff, @r#"
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
         sdist = { url = "https://files.pythonhosted.org/packages/d3/cc/8025ad5102a5c754023092143b8b511e184ec087dfbfb357d7d88fb82bff/twine-5.0.0.tar.gz", hash = "sha256:89b0cc7d370a4b66421cc6102f269aa910fe0f1861c124f573cf2ddedbc10cf4", size = 222119, upload-time = "2024-02-11T19:59:40.377Z" }
         wheels = [
             { url = "https://files.pythonhosted.org/packages/9a/d4/4db90c4a2b8c1006ea3e6291f36b50b66e45887cf17b3b958b5d646fb837/twine-5.0.0-py3-none-any.whl", hash = "sha256:a262933de0b484c53408f9edae2e7821c1c45a3314ff2df9bdd343aa7ab8edc0", size = 37138, upload-time = "2024-02-11T19:59:38.163Z" },
         ]

         [[package]]
        +name = "tzdata"
        +version = "2024.1"
        +source = { registry = "https://pypi.org/simple" }
        +sdist = { url = "https://files.pythonhosted.org/packages/74/5b/e025d02cb3b66b7b76093404392d4b44343c69101cc85f4d180dd5784717/tzdata-2024.1.tar.gz", hash = "sha256:2674120f8d891909751c38abcdfd386ac0a5a1127954fbc332af6b5ceae07efd", size = 190559, upload-time = "2024-02-11T23:22:40.2Z" }
        +wheels = [
        +    { url = "https://files.pythonhosted.org/packages/65/58/f9c9e6be752e9fcb8b6a0ee9fb87e6e7a1f6bcab2cdc73f02bb7ba91ada0/tzdata-2024.1-py2.py3-none-any.whl", hash = "sha256:9068bc196136463f5245e51efda838afa15aaeca9903f49050dfa2679db4d252", size = 345370, upload-time = "2024-02-11T23:22:38.223Z" },
        +]
        +
        +[[package]]
         name = "urllib3"
         version = "2.2.1"
         source = { registry = "https://pypi.org/simple" }
         sdist = { url = "https://files.pythonhosted.org/packages/7a/50/7fd50a27caa0652cd4caf224aa87741ea41d3265ad13f010886167cfcc79/urllib3-2.2.1.tar.gz", hash = "sha256:d0570876c61ab9e520d776c38acbbb5b05a776d3f9ff98a5c8fd5162a444cf19", size = 291020, upload-time = "2024-02-18T03:55:57.539Z" }
         wheels = [
             { url = "https://files.pythonhosted.org/packages/a2/73/a68704750a7679d0b6d3ad7aa8d4da8e14e151ae82e6fee774e6e0d05ec8/urllib3-2.2.1-py3-none-any.whl", hash = "sha256:450b20ec296a467077128bff42b73080516e71b56ff59a60a02bef2232c4fa9d", size = 121067, upload-time = "2024-02-18T03:55:54.704Z" },
         ]

         [[package]]
         name = "watchfiles"
        "#);
    });

    let diff = context.diff_lock(|context| {
        let mut remove_cmd = context.remove();
        remove_cmd.arg("--no-sync").arg("tzdata");
        remove_cmd
    });
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(diff, @r#"
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
         sdist = { url = "https://files.pythonhosted.org/packages/d3/cc/8025ad5102a5c754023092143b8b511e184ec087dfbfb357d7d88fb82bff/twine-5.0.0.tar.gz", hash = "sha256:89b0cc7d370a4b66421cc6102f269aa910fe0f1861c124f573cf2ddedbc10cf4", size = 222119, upload-time = "2024-02-11T19:59:40.377Z" }
         wheels = [
             { url = "https://files.pythonhosted.org/packages/9a/d4/4db90c4a2b8c1006ea3e6291f36b50b66e45887cf17b3b958b5d646fb837/twine-5.0.0-py3-none-any.whl", hash = "sha256:a262933de0b484c53408f9edae2e7821c1c45a3314ff2df9bdd343aa7ab8edc0", size = 37138, upload-time = "2024-02-11T19:59:38.163Z" },
        -]
        -
        -[[package]]
        -name = "tzdata"
        -version = "2024.1"
        -source = { registry = "https://pypi.org/simple" }
        -sdist = { url = "https://files.pythonhosted.org/packages/74/5b/e025d02cb3b66b7b76093404392d4b44343c69101cc85f4d180dd5784717/tzdata-2024.1.tar.gz", hash = "sha256:2674120f8d891909751c38abcdfd386ac0a5a1127954fbc332af6b5ceae07efd", size = 190559, upload-time = "2024-02-11T23:22:40.2Z" }
        -wheels = [
        -    { url = "https://files.pythonhosted.org/packages/65/58/f9c9e6be752e9fcb8b6a0ee9fb87e6e7a1f6bcab2cdc73f02bb7ba91ada0/tzdata-2024.1-py2.py3-none-any.whl", hash = "sha256:9068bc196136463f5245e51efda838afa15aaeca9903f49050dfa2679db4d252", size = 345370, upload-time = "2024-02-11T23:22:38.223Z" },
         ]

         [[package]]
         name = "urllib3"
         version = "2.2.1"
         source = { registry = "https://pypi.org/simple" }
         sdist = { url = "https://files.pythonhosted.org/packages/7a/50/7fd50a27caa0652cd4caf224aa87741ea41d3265ad13f010886167cfcc79/urllib3-2.2.1.tar.gz", hash = "sha256:d0570876c61ab9e520d776c38acbbb5b05a776d3f9ff98a5c8fd5162a444cf19", size = 291020, upload-time = "2024-02-18T03:55:57.539Z" }
         wheels = [
             { url = "https://files.pythonhosted.org/packages/a2/73/a68704750a7679d0b6d3ad7aa8d4da8e14e151ae82e6fee774e6e0d05ec8/urllib3-2.2.1-py3-none-any.whl", hash = "sha256:450b20ec296a467077128bff42b73080516e71b56ff59a60a02bef2232c4fa9d", size = 121067, upload-time = "2024-02-18T03:55:54.704Z" },
         ]
        "#);
    });

    // Back to where we started.
    let new_lock = context.read("uv.lock");
    let diff = diff_snapshot(&lock, &new_lock, 10);
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(diff, @"");
    });
}

#[test]
fn packse_add_remove_existing_package_noop() {
    let context = uv_test::test_context!("3.12");
    context.copy_ecosystem_project("packse");

    uv_snapshot!(context.filters(), context.lock(), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The `tool.uv.dev-dependencies` field (used in `pyproject.toml`) is deprecated and will be removed in a future release; use `dependency-groups.dev` instead
    Resolved 49 packages in [TIME]
    ");

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
    let context = uv_test::test_context!("3.12");
    context.copy_ecosystem_project("packse");

    uv_snapshot!(context.filters(), context.lock(), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The `tool.uv.dev-dependencies` field (used in `pyproject.toml`) is deprecated and will be removed in a future release; use `dependency-groups.dev` instead
    Resolved 49 packages in [TIME]
    ");

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
        assert_snapshot!(diff, @r#"
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
        "#);
    });

    let diff = context.diff_lock(|context| {
        let mut remove_cmd = context.remove();
        remove_cmd.arg("--no-sync").arg("sniffio");
        remove_cmd
    });
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(diff, @r#"
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
        "#);
    });

    // Back to where we started.
    let new_lock = context.read("uv.lock");
    let diff = diff_snapshot(&lock, &new_lock, 10);
    insta::with_settings!({
        filters => context.filters(),
    }, {
        assert_snapshot!(diff, @"");
    });
}

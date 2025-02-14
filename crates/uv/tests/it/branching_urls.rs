use std::env;

use anyhow::Result;
use indoc::indoc;
use insta::assert_snapshot;

use crate::common::{make_project, uv_snapshot, TestContext};

/// The root package has diverging URLs for disjoint markers:
/// ```toml
/// dependencies = [
///   "iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl ; python_version >= '3.12'",
///   "iniconfig @ https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl ; python_version < '3.12'",
/// ]
/// ```
#[test]
fn branching_urls_disjoint() -> Result<()> {
    let context = TestContext::new("3.12");

    let deps = indoc! {r#"
        dependencies = [
            # Valid, disjoint split
            "iniconfig @ https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl ; python_version < '3.12'",
            "iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl ; python_version >= '3.12'",
        ]
    "# };
    make_project(context.temp_dir.path(), "a", deps)?;

    uv_snapshot!(context.filters(), context.lock().current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###
    );

    Ok(())
}

/// The root package has diverging URLs, but their markers are not disjoint:
/// ```toml
/// dependencies = [
///   "iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl ; python_version >= '3.11'",
///   "iniconfig @ https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl ; python_version < '3.12'",
/// ]
/// ```
#[test]
fn branching_urls_overlapping() -> Result<()> {
    let context = TestContext::new("3.12");

    let deps = indoc! {r#"
        dependencies = [
            # Conflicting split
            "iniconfig @ https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl ; python_version < '3.12'",
            "iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl ; python_version >= '3.11'",
        ]
    "# };
    make_project(context.temp_dir.path(), "a", deps)?;

    uv_snapshot!(context.filters(), context.lock().current_dir(&context.temp_dir), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Requirements contain conflicting URLs for package `iniconfig` in split `python_full_version == '3.11.*'`:
    - https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl
    - https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl
    "###
    );

    Ok(())
}

/// The root package has diverging URLs, but transitive dependencies have conflicting URLs.
///
/// Requirements:
/// ```text
/// a -> anyio (allowed forking urls to force a split)
/// a -> b -> b1 -> https://../iniconfig-1.1.1-py3-none-any.whl
/// a -> b -> b2 -> https://../iniconfig-2.0.0-py3-none-any.whl
/// ```
#[test]
fn root_package_splits_but_transitive_conflict() -> Result<()> {
    let context = TestContext::new("3.12");

    let deps = indoc! {r#"
        dependencies = [
            # Force a split
            "anyio==4.3.0 ; python_version >= '3.12'",
            "anyio==4.2.0 ; python_version < '3.12'",
            "b"
        ]

        [tool.uv.sources]
        b = { path = "b" }
    "# };
    make_project(context.temp_dir.path(), "a", deps)?;

    let deps = indoc! {r#"
        dependencies = [
            "b1",
            "b2",
        ]

        [tool.uv.sources]
        b1 = { path = "../b1" }
        b2 = { path = "../b2" }
    "# };
    make_project(&context.temp_dir.path().join("b"), "b", deps)?;

    let deps = indoc! {r#"
        dependencies = [
            "iniconfig @ https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl",
        ]
    "# };
    make_project(&context.temp_dir.path().join("b1"), "b1", deps)?;

    let deps = indoc! {r#"
        dependencies = [
            "iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl",
        ]
    "# };
    make_project(&context.temp_dir.path().join("b2"), "b2", deps)?;

    uv_snapshot!(context.filters(), context.lock().current_dir(&context.temp_dir), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Requirements contain conflicting URLs for package `iniconfig` in split `python_full_version >= '3.12'`:
    - https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl
    - https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl
    "###
    );

    Ok(())
}

/// The root package has diverging URLs, and transitive dependencies through an intermediate
/// package have one URL for each side.
///
/// Requirements:
/// ```text
/// a -> anyio==4.4.0 ; python_version >= '3.12'
///  a -> anyio==4.3.0 ; python_version < '3.12'
/// a -> b -> b1 ; python_version < '3.12' -> https://../iniconfig-1.1.1-py3-none-any.whl
/// a -> b -> b2 ; python_version >= '3.12' -> https://../iniconfig-2.0.0-py3-none-any.whl
/// ```
#[test]
fn root_package_splits_transitive_too() -> Result<()> {
    let context = TestContext::new("3.12");

    let deps = indoc! {r#"
        dependencies = [
            # Force a split
            "anyio==4.3.0 ; python_version >= '3.12'",
            "anyio==4.2.0 ; python_version < '3.12'",
            "b"
        ]

        [tool.uv.sources]
        b = { path = "b" }
    "# };
    make_project(context.temp_dir.path(), "a", deps)?;

    let deps = indoc! {r#"
        dependencies = [
            "b1 ; python_version < '3.12'",
            "b2 ; python_version >= '3.12'",
        ]

        [tool.uv.sources]
        b1 = { path = "../b1" }
        b2 = { path = "../b2" }
    "# };
    make_project(&context.temp_dir.path().join("b"), "b", deps)?;

    let deps = indoc! {r#"
        dependencies = [
            "iniconfig @ https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl",
        ]
    "# };
    make_project(&context.temp_dir.path().join("b1"), "b1", deps)?;

    let deps = indoc! {r#"
        dependencies = [
            "iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl",
        ]
    "# };
    make_project(&context.temp_dir.path().join("b2"), "b2", deps)?;

    uv_snapshot!(context.filters(), context.lock().current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    "###
    );

    assert_snapshot!(context.read("uv.lock"), @r###"
    version = 1
    revision = 1
    requires-python = ">=3.11, <3.13"
    resolution-markers = [
        "python_full_version >= '3.12'",
        "python_full_version < '3.12'",
    ]

    [options]
    exclude-newer = "2024-03-25T00:00:00Z"

    [[package]]
    name = "a"
    version = "0.1.0"
    source = { editable = "." }
    dependencies = [
        { name = "anyio", version = "4.2.0", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version < '3.12'" },
        { name = "anyio", version = "4.3.0", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version >= '3.12'" },
        { name = "b" },
    ]

    [package.metadata]
    requires-dist = [
        { name = "anyio", marker = "python_full_version < '3.12'", specifier = "==4.2.0" },
        { name = "anyio", marker = "python_full_version >= '3.12'", specifier = "==4.3.0" },
        { name = "b", directory = "b" },
    ]

    [[package]]
    name = "anyio"
    version = "4.2.0"
    source = { registry = "https://pypi.org/simple" }
    resolution-markers = [
        "python_full_version < '3.12'",
    ]
    dependencies = [
        { name = "idna", marker = "python_full_version < '3.12'" },
        { name = "sniffio", marker = "python_full_version < '3.12'" },
    ]
    sdist = { url = "https://files.pythonhosted.org/packages/2d/b8/7333d87d5f03247215d86a86362fd3e324111788c6cdd8d2e6196a6ba833/anyio-4.2.0.tar.gz", hash = "sha256:e1875bb4b4e2de1669f4bc7869b6d3f54231cdced71605e6e64c9be77e3be50f", size = 158770 }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/bf/cd/d6d9bb1dadf73e7af02d18225cbd2c93f8552e13130484f1c8dcfece292b/anyio-4.2.0-py3-none-any.whl", hash = "sha256:745843b39e829e108e518c489b31dc757de7d2131d53fac32bd8df268227bfee", size = 85481 },
    ]

    [[package]]
    name = "anyio"
    version = "4.3.0"
    source = { registry = "https://pypi.org/simple" }
    resolution-markers = [
        "python_full_version >= '3.12'",
    ]
    dependencies = [
        { name = "idna", marker = "python_full_version >= '3.12'" },
        { name = "sniffio", marker = "python_full_version >= '3.12'" },
    ]
    sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
    ]

    [[package]]
    name = "b"
    version = "0.1.0"
    source = { directory = "b" }
    dependencies = [
        { name = "b1", marker = "python_full_version < '3.12'" },
        { name = "b2", marker = "python_full_version >= '3.12'" },
    ]

    [package.metadata]
    requires-dist = [
        { name = "b1", marker = "python_full_version < '3.12'", directory = "b1" },
        { name = "b2", marker = "python_full_version >= '3.12'", directory = "b2" },
    ]

    [[package]]
    name = "b1"
    version = "0.1.0"
    source = { directory = "b1" }
    dependencies = [
        { name = "iniconfig", version = "1.1.1", source = { url = "https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl" }, marker = "python_full_version < '3.12'" },
    ]

    [package.metadata]
    requires-dist = [{ name = "iniconfig", url = "https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl" }]

    [[package]]
    name = "b2"
    version = "0.1.0"
    source = { directory = "b2" }
    dependencies = [
        { name = "iniconfig", version = "2.0.0", source = { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl" }, marker = "python_full_version >= '3.12'" },
    ]

    [package.metadata]
    requires-dist = [{ name = "iniconfig", url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl" }]

    [[package]]
    name = "idna"
    version = "3.6"
    source = { registry = "https://pypi.org/simple" }
    sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
    ]

    [[package]]
    name = "iniconfig"
    version = "1.1.1"
    source = { url = "https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl" }
    resolution-markers = [
        "python_full_version < '3.12'",
    ]
    wheels = [
        { url = "https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl", hash = "sha256:011e24c64b7f47f6ebd835bb12a743f2fbe9a26d4cecaa7f53bc4f35ee9da8b3" },
    ]

    [[package]]
    name = "iniconfig"
    version = "2.0.0"
    source = { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl" }
    resolution-markers = [
        "python_full_version >= '3.12'",
    ]
    wheels = [
        { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374" },
    ]

    [[package]]
    name = "sniffio"
    version = "1.3.1"
    source = { registry = "https://pypi.org/simple" }
    sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
    ]
    "###);

    Ok(())
}

/// The root package has diverging URLs on one package, and other dependencies have one URL
/// for each side.
///
/// Requirements:
/// ```
/// a -> anyio==4.4.0 ; python_version >= '3.12'
/// a -> anyio==4.3.0 ; python_version < '3.12'
/// a -> b1 ; python_version < '3.12' -> iniconfig==1.1.1
/// a -> b2 ; python_version >= '3.12' -> iniconfig==2.0.0
/// ```
#[test]
fn root_package_splits_other_dependencies_too() -> Result<()> {
    let context = TestContext::new("3.12");

    let deps = indoc! {r#"
        dependencies = [
            # Force a split
            "anyio==4.3.0 ; python_version >= '3.12'",
            "anyio==4.2.0 ; python_version < '3.12'",
            # These two are currently included in both parts of the split.
            "b1 ; python_version < '3.12'",
            "b2 ; python_version >= '3.12'",
        ]

        [tool.uv.sources]
        b1 = { path = "b1" }
        b2 = { path = "b2" }
    "# };
    make_project(context.temp_dir.path(), "a", deps)?;

    let deps = indoc! {r#"
        dependencies = [
            "iniconfig==1.1.1",
        ]
    "# };
    make_project(&context.temp_dir.path().join("b1"), "b1", deps)?;

    let deps = indoc! {r#"
        dependencies = [
            "iniconfig==2.0.0"
        ]
    "# };
    make_project(&context.temp_dir.path().join("b2"), "b2", deps)?;

    uv_snapshot!(context.filters(), context.lock().current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 9 packages in [TIME]
    "###
    );

    assert_snapshot!(context.read("uv.lock"), @r#"
    version = 1
    revision = 1
    requires-python = ">=3.11, <3.13"
    resolution-markers = [
        "python_full_version >= '3.12'",
        "python_full_version < '3.12'",
    ]

    [options]
    exclude-newer = "2024-03-25T00:00:00Z"

    [[package]]
    name = "a"
    version = "0.1.0"
    source = { editable = "." }
    dependencies = [
        { name = "anyio", version = "4.2.0", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version < '3.12'" },
        { name = "anyio", version = "4.3.0", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version >= '3.12'" },
        { name = "b1", marker = "python_full_version < '3.12'" },
        { name = "b2", marker = "python_full_version >= '3.12'" },
    ]

    [package.metadata]
    requires-dist = [
        { name = "anyio", marker = "python_full_version < '3.12'", specifier = "==4.2.0" },
        { name = "anyio", marker = "python_full_version >= '3.12'", specifier = "==4.3.0" },
        { name = "b1", marker = "python_full_version < '3.12'", directory = "b1" },
        { name = "b2", marker = "python_full_version >= '3.12'", directory = "b2" },
    ]

    [[package]]
    name = "anyio"
    version = "4.2.0"
    source = { registry = "https://pypi.org/simple" }
    resolution-markers = [
        "python_full_version < '3.12'",
    ]
    dependencies = [
        { name = "idna", marker = "python_full_version < '3.12'" },
        { name = "sniffio", marker = "python_full_version < '3.12'" },
    ]
    sdist = { url = "https://files.pythonhosted.org/packages/2d/b8/7333d87d5f03247215d86a86362fd3e324111788c6cdd8d2e6196a6ba833/anyio-4.2.0.tar.gz", hash = "sha256:e1875bb4b4e2de1669f4bc7869b6d3f54231cdced71605e6e64c9be77e3be50f", size = 158770 }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/bf/cd/d6d9bb1dadf73e7af02d18225cbd2c93f8552e13130484f1c8dcfece292b/anyio-4.2.0-py3-none-any.whl", hash = "sha256:745843b39e829e108e518c489b31dc757de7d2131d53fac32bd8df268227bfee", size = 85481 },
    ]

    [[package]]
    name = "anyio"
    version = "4.3.0"
    source = { registry = "https://pypi.org/simple" }
    resolution-markers = [
        "python_full_version >= '3.12'",
    ]
    dependencies = [
        { name = "idna", marker = "python_full_version >= '3.12'" },
        { name = "sniffio", marker = "python_full_version >= '3.12'" },
    ]
    sdist = { url = "https://files.pythonhosted.org/packages/db/4d/3970183622f0330d3c23d9b8a5f52e365e50381fd484d08e3285104333d3/anyio-4.3.0.tar.gz", hash = "sha256:f75253795a87df48568485fd18cdd2a3fa5c4f7c5be8e5e36637733fce06fed6", size = 159642 }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/14/fd/2f20c40b45e4fb4324834aea24bd4afdf1143390242c0b33774da0e2e34f/anyio-4.3.0-py3-none-any.whl", hash = "sha256:048e05d0f6caeed70d731f3db756d35dcc1f35747c8c403364a8332c630441b8", size = 85584 },
    ]

    [[package]]
    name = "b1"
    version = "0.1.0"
    source = { directory = "b1" }
    dependencies = [
        { name = "iniconfig", version = "1.1.1", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version < '3.12'" },
    ]

    [package.metadata]
    requires-dist = [{ name = "iniconfig", specifier = "==1.1.1" }]

    [[package]]
    name = "b2"
    version = "0.1.0"
    source = { directory = "b2" }
    dependencies = [
        { name = "iniconfig", version = "2.0.0", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version >= '3.12'" },
    ]

    [package.metadata]
    requires-dist = [{ name = "iniconfig", specifier = "==2.0.0" }]

    [[package]]
    name = "idna"
    version = "3.6"
    source = { registry = "https://pypi.org/simple" }
    sdist = { url = "https://files.pythonhosted.org/packages/bf/3f/ea4b9117521a1e9c50344b909be7886dd00a519552724809bb1f486986c2/idna-3.6.tar.gz", hash = "sha256:9ecdbbd083b06798ae1e86adcbfe8ab1479cf864e4ee30fe4e46a003d12491ca", size = 175426 }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/c2/e7/a82b05cf63a603df6e68d59ae6a68bf5064484a0718ea5033660af4b54a9/idna-3.6-py3-none-any.whl", hash = "sha256:c05567e9c24a6b9faaa835c4821bad0590fbb9d5779e7caa6e1cc4978e7eb24f", size = 61567 },
    ]

    [[package]]
    name = "iniconfig"
    version = "1.1.1"
    source = { registry = "https://pypi.org/simple" }
    resolution-markers = [
        "python_full_version < '3.12'",
    ]
    sdist = { url = "https://files.pythonhosted.org/packages/23/a2/97899f6bd0e873fed3a7e67ae8d3a08b21799430fb4da15cfedf10d6e2c2/iniconfig-1.1.1.tar.gz", hash = "sha256:bc3af051d7d14b2ee5ef9969666def0cd1a000e121eaea580d4a313df4b37f32", size = 8104 }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl", hash = "sha256:011e24c64b7f47f6ebd835bb12a743f2fbe9a26d4cecaa7f53bc4f35ee9da8b3", size = 4990 },
    ]

    [[package]]
    name = "iniconfig"
    version = "2.0.0"
    source = { registry = "https://pypi.org/simple" }
    resolution-markers = [
        "python_full_version >= '3.12'",
    ]
    sdist = { url = "https://files.pythonhosted.org/packages/d7/4b/cbd8e699e64a6f16ca3a8220661b5f83792b3017d0f79807cb8708d33913/iniconfig-2.0.0.tar.gz", hash = "sha256:2d91e135bf72d31a410b17c16da610a82cb55f6b0477d1a902134b24a455b8b3", size = 4646 }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374", size = 5892 },
    ]

    [[package]]
    name = "sniffio"
    version = "1.3.1"
    source = { registry = "https://pypi.org/simple" }
    sdist = { url = "https://files.pythonhosted.org/packages/a2/87/a6771e1546d97e7e041b6ae58d80074f81b7d5121207425c964ddf5cfdbd/sniffio-1.3.1.tar.gz", hash = "sha256:f4324edc670a0f49750a81b895f35c3adb843cca46f0530f79fc1babb23789dc", size = 20372 }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/e9/44/75a9c9421471a6c4805dbf2356f7c181a29c1879239abab1ea2cc8f38b40/sniffio-1.3.1-py3-none-any.whl", hash = "sha256:2f6da418d1f1e0fddd844478f41680e794e6051915791a034ff65e5f100525a2", size = 10235 },
    ]
    "#);

    Ok(())
}

/// Whether the dependency comes from the registry or a direct URL depends on the branch.
///
/// ```toml
/// dependencies = [
///   "iniconfig == 1.1.1 ; python_version < '3.12'",
///   "iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl ; python_version >= '3.12'",
/// ]
/// ```
#[test]
fn branching_between_registry_and_direct_url() -> Result<()> {
    let context = TestContext::new("3.12");

    let deps = indoc! {r#"
        dependencies = [
            "iniconfig == 1.1.1 ; python_version < '3.12'",
            "iniconfig @ https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl ; python_version >= '3.12'",
        ]
    "# };
    make_project(context.temp_dir.path(), "a", deps)?;

    uv_snapshot!(context.filters(), context.lock().current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###
    );

    // We have source dist and wheel for the registry, but only the wheel for the direct URL.
    assert_snapshot!(context.read("uv.lock"), @r###"
    version = 1
    revision = 1
    requires-python = ">=3.11, <3.13"
    resolution-markers = [
        "python_full_version >= '3.12'",
        "python_full_version < '3.12'",
    ]

    [options]
    exclude-newer = "2024-03-25T00:00:00Z"

    [[package]]
    name = "a"
    version = "0.1.0"
    source = { editable = "." }
    dependencies = [
        { name = "iniconfig", version = "1.1.1", source = { registry = "https://pypi.org/simple" }, marker = "python_full_version < '3.12'" },
        { name = "iniconfig", version = "2.0.0", source = { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl" }, marker = "python_full_version >= '3.12'" },
    ]

    [package.metadata]
    requires-dist = [
        { name = "iniconfig", marker = "python_full_version < '3.12'", specifier = "==1.1.1" },
        { name = "iniconfig", marker = "python_full_version >= '3.12'", url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl" },
    ]

    [[package]]
    name = "iniconfig"
    version = "1.1.1"
    source = { registry = "https://pypi.org/simple" }
    resolution-markers = [
        "python_full_version < '3.12'",
    ]
    sdist = { url = "https://files.pythonhosted.org/packages/23/a2/97899f6bd0e873fed3a7e67ae8d3a08b21799430fb4da15cfedf10d6e2c2/iniconfig-1.1.1.tar.gz", hash = "sha256:bc3af051d7d14b2ee5ef9969666def0cd1a000e121eaea580d4a313df4b37f32", size = 8104 }
    wheels = [
        { url = "https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl", hash = "sha256:011e24c64b7f47f6ebd835bb12a743f2fbe9a26d4cecaa7f53bc4f35ee9da8b3", size = 4990 },
    ]

    [[package]]
    name = "iniconfig"
    version = "2.0.0"
    source = { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl" }
    resolution-markers = [
        "python_full_version >= '3.12'",
    ]
    wheels = [
        { url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374" },
    ]
    "###);

    Ok(())
}

/// The root package has two different direct URLs for disjoint forks, but they are from different sources.
///
/// ```toml
/// dependencies = [
///   "iniconfig @ https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl ; python_version < '3.12'",
///   "iniconfig @ git+https://github.com/pytest-dev/iniconfig@93f5930e668c0d1ddf4597e38dd0dea4e2665e7a ; python_version >= '3.12'",
/// ]
/// ```
#[test]
#[cfg(feature = "git")]
fn branching_urls_of_different_sources_disjoint() -> Result<()> {
    let context = TestContext::new("3.12");

    let deps = indoc! {r#"
        dependencies = [
            # Valid, disjoint split
            "iniconfig @ https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl ; python_version < '3.12'",
            "iniconfig @ git+https://github.com/pytest-dev/iniconfig@93f5930e668c0d1ddf4597e38dd0dea4e2665e7a ; python_version >= '3.12'",
        ]
    "# };
    make_project(context.temp_dir.path(), "a", deps)?;

    uv_snapshot!(context.filters(), context.lock().current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###
    );

    // We have source dist and wheel for the registry, but only the wheel for the direct URL.
    assert_snapshot!(context.read("uv.lock"), @r###"
    version = 1
    revision = 1
    requires-python = ">=3.11, <3.13"
    resolution-markers = [
        "python_full_version >= '3.12'",
        "python_full_version < '3.12'",
    ]

    [options]
    exclude-newer = "2024-03-25T00:00:00Z"

    [[package]]
    name = "a"
    version = "0.1.0"
    source = { editable = "." }
    dependencies = [
        { name = "iniconfig", version = "1.1.1", source = { url = "https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl" }, marker = "python_full_version < '3.12'" },
        { name = "iniconfig", version = "2.0.0", source = { git = "https://github.com/pytest-dev/iniconfig?rev=93f5930e668c0d1ddf4597e38dd0dea4e2665e7a#93f5930e668c0d1ddf4597e38dd0dea4e2665e7a" }, marker = "python_full_version >= '3.12'" },
    ]

    [package.metadata]
    requires-dist = [
        { name = "iniconfig", marker = "python_full_version < '3.12'", url = "https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl" },
        { name = "iniconfig", marker = "python_full_version >= '3.12'", git = "https://github.com/pytest-dev/iniconfig?rev=93f5930e668c0d1ddf4597e38dd0dea4e2665e7a" },
    ]

    [[package]]
    name = "iniconfig"
    version = "1.1.1"
    source = { url = "https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl" }
    resolution-markers = [
        "python_full_version < '3.12'",
    ]
    wheels = [
        { url = "https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl", hash = "sha256:011e24c64b7f47f6ebd835bb12a743f2fbe9a26d4cecaa7f53bc4f35ee9da8b3" },
    ]

    [[package]]
    name = "iniconfig"
    version = "2.0.0"
    source = { git = "https://github.com/pytest-dev/iniconfig?rev=93f5930e668c0d1ddf4597e38dd0dea4e2665e7a#93f5930e668c0d1ddf4597e38dd0dea4e2665e7a" }
    resolution-markers = [
        "python_full_version >= '3.12'",
    ]
    "###);

    Ok(())
}

/// The root package has two different direct URLs from different sources, but they are not
/// disjoint.
///
/// ```toml
/// dependencies = [
///   "iniconfig @ https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl ; python_version < '3.12'",
///   "iniconfig @ git+https://github.com/pytest-dev/iniconfig@93f5930e668c0d1ddf4597e38dd0dea4e2665e7a ; python_version >= '3.12'",
/// ]
/// ```
#[test]
#[cfg(feature = "git")]
fn branching_urls_of_different_sources_conflict() -> Result<()> {
    let context = TestContext::new("3.12");

    let deps = indoc! {r#"
        dependencies = [
            # Conflicting split
            "iniconfig @ https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl ; python_version < '3.12'",
            "iniconfig @ git+https://github.com/pytest-dev/iniconfig@93f5930e668c0d1ddf4597e38dd0dea4e2665e7a ; python_version >= '3.11'",
        ]
    "# };
    make_project(context.temp_dir.path(), "a", deps)?;

    uv_snapshot!(context.filters(), context.lock().current_dir(&context.temp_dir), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Requirements contain conflicting URLs for package `iniconfig` in split `python_full_version == '3.11.*'`:
    - git+https://github.com/pytest-dev/iniconfig@93f5930e668c0d1ddf4597e38dd0dea4e2665e7a
    - https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl
    "###
    );

    Ok(())
}

/// Ensure that we don't pre-visit package with URLs.
#[test]
fn dont_pre_visit_url_packages() -> Result<()> {
    let context = TestContext::new("3.12");

    let deps = indoc! {r#"
        dependencies = [
            # This c is not a registry distribution, we must not pre-visit it as such.
            "c==0.1.0",
            "b",
        ]

        [tool.uv.sources]
        b = { path = "b" }
    "# };
    make_project(context.temp_dir.path(), "a", deps)?;

    let deps = indoc! {r#"
        dependencies = [
          "c",
        ]

        [tool.uv.sources]
        c = { path = "../c" }
    "# };
    make_project(&context.temp_dir.join("b"), "b", deps)?;
    let deps = indoc! {r"
        dependencies = []
    " };
    make_project(&context.temp_dir.join("c"), "c", deps)?;

    uv_snapshot!(context.filters(), context.lock().arg("--offline").current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###
    );

    assert_snapshot!(context.read("uv.lock"), @r###"
    version = 1
    revision = 1
    requires-python = ">=3.11, <3.13"

    [options]
    exclude-newer = "2024-03-25T00:00:00Z"

    [[package]]
    name = "a"
    version = "0.1.0"
    source = { editable = "." }
    dependencies = [
        { name = "b" },
        { name = "c" },
    ]

    [package.metadata]
    requires-dist = [
        { name = "b", directory = "b" },
        { name = "c", specifier = "==0.1.0" },
    ]

    [[package]]
    name = "b"
    version = "0.1.0"
    source = { directory = "b" }
    dependencies = [
        { name = "c" },
    ]

    [package.metadata]
    requires-dist = [{ name = "c", directory = "c" }]

    [[package]]
    name = "c"
    version = "0.1.0"
    source = { directory = "c" }
    "###);

    Ok(())
}

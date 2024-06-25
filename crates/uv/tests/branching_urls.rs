use std::env;
use std::path::Path;

use anyhow::Result;
use indoc::{formatdoc, indoc};
use insta::assert_snapshot;

use crate::common::{uv_snapshot, TestContext};

mod common;

/// Create a stub package `name` in `dir` with the given `pyproject.toml` body.
fn make_project(dir: &Path, name: &str, body: &str) -> Result<()> {
    let pyproject_toml = formatdoc! {r#"
        [project]
        name = "{name}"
        version = "0.1.0"
        description = "Test package for direct URLs in branches"
        requires-python = ">=3.11,<3.13"
        {body}

        [build-system]
        requires = ["flit_core>=3.8,<4"]
        build-backend = "flit_core.buildapi"
        "#
    };
    fs_err::create_dir_all(dir)?;
    fs_err::write(dir.join("pyproject.toml"), pyproject_toml)?;
    fs_err::create_dir(dir.join(name))?;
    fs_err::write(dir.join(name).join("__init__.py"), "")?;
    Ok(())
}

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

    uv_snapshot!(context.filters(), context.lock().arg("--preview").current_dir(&context.temp_dir), @r###"
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

    uv_snapshot!(context.filters(), context.lock().arg("--preview").current_dir(&context.temp_dir), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Requirements contain conflicting URLs for package `iniconfig`:
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

    uv_snapshot!(context.filters(), context.lock().arg("--preview").current_dir(&context.temp_dir), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Requirements contain conflicting URLs for package `iniconfig` in split `python_version < '3.12'`:
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

    uv_snapshot!(context.filters(), context.lock().arg("--preview").current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 10 packages in [TIME]
    "###
    );

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

    uv_snapshot!(context.filters(), context.lock().arg("--preview").current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 9 packages in [TIME]
    "###
    );

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

    uv_snapshot!(context.filters(), context.lock().arg("--preview").current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###
    );

    // We have source dist and wheel for the registry, but only the wheel for the direct URL.
    assert_snapshot!(fs_err::read_to_string(context.temp_dir.join("uv.lock"))?, @r###"
    version = 1
    requires-python = ">=3.11, <3.13"

    [[distribution]]
    name = "a"
    version = "0.1.0"
    source = "editable+."

    [[distribution.dependencies]]
    name = "iniconfig"
    version = "1.1.1"
    source = "registry+https://pypi.org/simple"
    marker = "python_version < '3.12'"

    [[distribution.dependencies]]
    name = "iniconfig"
    version = "2.0.0"
    source = "direct+https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl"
    marker = "python_version >= '3.12'"

    [[distribution]]
    name = "iniconfig"
    version = "1.1.1"
    source = "registry+https://pypi.org/simple"
    sdist = { url = "https://files.pythonhosted.org/packages/23/a2/97899f6bd0e873fed3a7e67ae8d3a08b21799430fb4da15cfedf10d6e2c2/iniconfig-1.1.1.tar.gz", hash = "sha256:bc3af051d7d14b2ee5ef9969666def0cd1a000e121eaea580d4a313df4b37f32", size = 8104 }
    wheels = [{ url = "https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl", hash = "sha256:011e24c64b7f47f6ebd835bb12a743f2fbe9a26d4cecaa7f53bc4f35ee9da8b3", size = 4990 }]

    [[distribution]]
    name = "iniconfig"
    version = "2.0.0"
    source = "direct+https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl"
    wheels = [{ url = "https://files.pythonhosted.org/packages/ef/a6/62565a6e1cf69e10f5727360368e451d4b7f58beeac6173dc9db836a5b46/iniconfig-2.0.0-py3-none-any.whl", hash = "sha256:b6a85871a79d2e3b22d2d1b94ac2824226a63c6b741c88f7ae975f18b6778374" }]
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

    uv_snapshot!(context.filters(), context.lock().arg("--preview").current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###
    );

    // We have source dist and wheel for the registry, but only the wheel for the direct URL.
    assert_snapshot!(fs_err::read_to_string(context.temp_dir.join("uv.lock"))?, @r###"
    version = 1
    requires-python = ">=3.11, <3.13"

    [[distribution]]
    name = "a"
    version = "0.1.0"
    source = "editable+."

    [[distribution.dependencies]]
    name = "iniconfig"
    version = "1.1.1"
    source = "direct+https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl"
    marker = "python_version < '3.12'"

    [[distribution.dependencies]]
    name = "iniconfig"
    version = "2.0.0"
    source = "git+https://github.com/pytest-dev/iniconfig?rev=93f5930e668c0d1ddf4597e38dd0dea4e2665e7a#93f5930e668c0d1ddf4597e38dd0dea4e2665e7a"
    marker = "python_version >= '3.12'"

    [[distribution]]
    name = "iniconfig"
    version = "1.1.1"
    source = "direct+https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl"
    wheels = [{ url = "https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl", hash = "sha256:011e24c64b7f47f6ebd835bb12a743f2fbe9a26d4cecaa7f53bc4f35ee9da8b3" }]

    [[distribution]]
    name = "iniconfig"
    version = "2.0.0"
    source = "git+https://github.com/pytest-dev/iniconfig?rev=93f5930e668c0d1ddf4597e38dd0dea4e2665e7a#93f5930e668c0d1ddf4597e38dd0dea4e2665e7a"
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

    uv_snapshot!(context.filters(), context.lock().arg("--preview").current_dir(&context.temp_dir), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Requirements contain conflicting URLs for package `iniconfig`:
    - git+https://github.com/pytest-dev/iniconfig@93f5930e668c0d1ddf4597e38dd0dea4e2665e7a
    - https://files.pythonhosted.org/packages/9b/dd/b3c12c6d707058fa947864b67f0c4e0c39ef8610988d7baea9578f3c48f3/iniconfig-1.1.1-py2.py3-none-any.whl
    "###
    );

    Ok(())
}

/// Ensure that we don't pre-visit package with URLs.
#[test]
fn dont_previsit_url_packages() -> Result<()> {
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

    uv_snapshot!(context.filters(), context.lock().arg("--preview").arg("--offline").current_dir(&context.temp_dir), @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Resolved 3 packages in [TIME]
    "###
    );

    Ok(())
}

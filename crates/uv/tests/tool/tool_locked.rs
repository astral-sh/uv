use std::collections::BTreeMap;

#[cfg(feature = "test-pypi")]
use anyhow::Context;
use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::FileWriteStr;
use assert_fs::fixture::{FileWriteBin, PathChild, PathCreateDir};
use indoc::formatdoc;
use insta::allow_duplicates;
use serde_json::json;
use sha2::{Digest, Sha256};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

use uv_static::EnvVars;
use uv_test::packse::generate_wheel_with_files;
use uv_test::{TestContext, uv_snapshot};

#[cfg(feature = "test-pypi")]
fn dependency(context: &TestContext, version: &str) -> Result<String> {
    context
        .temp_dir
        .child("requirements.in")
        .write_str(&format!("idna=={version}"))?;
    context
        .pip_compile()
        .args(["requirements.in", "--no-deps", "-o", "pylock.toml"])
        .assert()
        .success();
    Ok(context.read("pylock.toml"))
}

fn tool(
    context: &TestContext,
    version: &str,
    lock: Option<&str>,
    python: Option<&str>,
) -> Result<()> {
    let pylock_path = format!("locked_tool-{version}.dist-info/pylock.toml");
    let entrypoints_path = format!("locked_tool-{version}.dist-info/entry_points.txt");
    let mut files = vec![
        (
            entrypoints_path.as_str(),
            "[console_scripts]\nlocked-tool = locked_tool.cli:main\n",
        ),
        (
            "locked_tool/cli.py",
            "from importlib.metadata import version\ndef main(): print(version('idna'))\n",
        ),
    ];
    if let Some(lock) = lock {
        files.push((&pylock_path, lock));
    }
    let (filename, bytes) = generate_wheel_with_files(
        &"locked-tool".parse()?,
        &version.parse()?,
        &["idna>=3.3,<3.5".parse()?],
        &BTreeMap::new(),
        python.map(str::parse).transpose()?.as_ref(),
        "py3-none-any",
        &files,
    );
    context
        .temp_dir
        .child("wheels")
        .child(filename)
        .write_binary(&bytes)?;
    Ok(())
}

#[tokio::test]
async fn packaged_lock_preserves_tool_url_hash() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_tool_dirs();
    context.temp_dir.child("wheels").create_dir_all()?;
    let (filename, bytes) = generate_wheel_with_files(
        &"locked-tool".parse()?,
        &"1.0.0".parse()?,
        &[],
        &BTreeMap::new(),
        None,
        "py3-none-any",
        &[
            (
                "locked_tool-1.0.0.dist-info/pylock.toml",
                "lock-version = \"1.0\"\ncreated-by = \"test\"\npackages = []\n",
            ),
            (
                "locked_tool-1.0.0.dist-info/entry_points.txt",
                "[console_scripts]\nlocked-tool = locked_tool.cli:main\n",
            ),
            ("locked_tool/cli.py", "def main(): pass\n"),
        ],
    );
    context
        .temp_dir
        .child("wheels")
        .child(filename)
        .write_binary(&bytes)?;
    let wheel = context
        .temp_dir
        .child("wheels/locked_tool-1.0.0-py3-none-any.whl");
    let hash = hex::encode(Sha256::digest(fs_err::read(wheel.path())?));
    let wheel_url = url::Url::from_file_path(wheel.path())
        .map_err(|()| anyhow::anyhow!("Could not create a file URL"))?;
    let requirement = format!("locked-tool @ {wheel_url}#sha256={hash}&egg=locked-tool");
    context
        .tool_install()
        .args([
            "--locked",
            "--preview-features",
            "locked-tools",
            "--from",
            &requirement,
            "locked-tool",
        ])
        .env(EnvVars::PATH, context.temp_dir.child("bin").path())
        .assert()
        .success();
    let receipt = context.read("tools/locked-tool/uv-receipt.toml");
    assert!(receipt.contains(&hash));
    assert!(!receipt.contains("egg=locked-tool"));
    Ok(())
}

#[tokio::test]
async fn packaged_lock_configured_index_and_constraints() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix()
        .with_tool_dirs();
    context.temp_dir.child("wheels").create_dir_all()?;
    let server = MockServer::start().await;
    let index = format!("{}/simple", server.uri());
    let (filename, wheel) = generate_wheel_with_files(
        &"idna".parse()?,
        &"3.3".parse()?,
        &[],
        &BTreeMap::new(),
        None,
        "py3-none-any",
        &[],
    );
    let hash = hex::encode(Sha256::digest(&wheel));
    let url = format!("{}/files/{filename}", server.uri());
    Mock::given(method("GET"))
        .and(path("/simple/idna/"))
        .respond_with(
            ResponseTemplate::new(200).insert_header("cache-control", "max-age=0").set_body_raw(
                json!({
                    "meta": { "api-version": "1.1" },
                    "name": "idna",
                    "files": [{ "filename": filename, "url": url, "hashes": { "sha256": hash } }]
                })
                .to_string(),
                "application/vnd.pypi.simple.v1+json",
            ),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/files/{filename}")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(wheel))
        .mount(&server)
        .await;
    let lock = formatdoc! {r#"
        lock-version = "1.0"
        created-by = "test"
        [[packages]]
        name = "idna"
        version = "3.3"
        index = "{index}"
        wheels = [{{ url = "{url}", hashes = {{ sha256 = "{hash}" }} }}]
    "#};
    tool(&context, "1.0.0", Some(&lock), None)?;
    let (different_filename, different_wheel) = generate_wheel_with_files(
        &"idna".parse()?,
        &"3.3".parse()?,
        &[],
        &BTreeMap::new(),
        None,
        "py3-none-any",
        &[("idna/private.py", "")],
    );
    context
        .temp_dir
        .child("wheels")
        .child(different_filename)
        .write_binary(&different_wheel)?;
    let bin_dir = context.temp_dir.child("bin");
    context
        .tool_install()
        .args(["--no-index", "--find-links", "wheels", "locked-tool"])
        .env(EnvVars::PATH, bin_dir.as_os_str())
        .assert()
        .success();
    let private_module = if cfg!(windows) {
        "tools/locked-tool/Lib/site-packages/idna/private.py"
    } else {
        "tools/locked-tool/lib/python3.12/site-packages/idna/private.py"
    };
    assert!(context.temp_dir.child(private_module).path().exists());
    context
        .temp_dir
        .child("constraints.txt")
        .write_str(&format!("idna==3.3 --hash=sha256:{hash}\n"))?;
    context
        .tool_install()
        .args([
            "--preview-features",
            "locked-tools",
            "--find-links",
            "wheels",
            "--index-url",
            &index,
            "--constraint",
            "constraints.txt",
            "locked-tool",
        ])
        .env(EnvVars::UV_TOOL_LOCKED, "1")
        .env(EnvVars::PATH, bin_dir.as_os_str())
        .assert()
        .success();
    assert!(
        context
            .read("tools/locked-tool/uv-receipt.toml")
            .contains("locked = true")
    );
    assert!(!context.temp_dir.child(private_module).path().exists());
    assert!(
        context
            .read("tools/locked-tool/uv-receipt.toml")
            .contains(&hash)
    );
    Ok(())
}

#[test]
fn packaged_lock_non_registry_sources() -> Result<()> {
    // None of these sources may be accessed while validating a packaged lock.
    for source in [
        r#"archive = { url = "https://example.invalid/idna-3.3.tar.gz", hashes = { sha256 = "0000000000000000000000000000000000000000000000000000000000000000" } }"#,
        r#"directory = { path = "dependency" }"#,
        r#"vcs = { type = "git", url = "https://example.invalid/idna", commit-id = "0123456789012345678901234567890123456789" }"#,
        r#"wheels = [{ url = "https://files.pythonhosted.org/idna-3.3-py3-none-any.whl", path = "idna-3.3-py3-none-any.whl", hashes = { sha256 = "0000000000000000000000000000000000000000000000000000000000000000" } }]"#,
        r#"sdist = { path = "idna-3.3.tar.gz", hashes = { sha256 = "0000000000000000000000000000000000000000000000000000000000000000" } }"#,
    ] {
        let context = uv_test::test_context!("3.12").with_tool_dirs();
        context.temp_dir.child("wheels").create_dir_all()?;
        let lock = formatdoc! {r#"
            lock-version = "1.0"
            created-by = "test"
            [[packages]]
            name = "idna"
            version = "3.3"
            {source}
        "#};
        tool(&context, "1.0.0", Some(&lock), None)?;
        allow_duplicates! {
            uv_snapshot!(context.filters(), context.tool_install()
                .args(["--locked", "--preview-features", "locked-tools", "--no-index", "--find-links", "wheels", "locked-tool==1.0.0"]), @"
            exit_code: 2 (failure)
            ----- stderr -----
            error: The packaged lock for `locked-tool==1.0.0` contains unverified artifacts
              cause: `idna==3.3` must use wheel or source distribution URLs from an index
            ");
        }
    }
    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn packaged_lock_artifact_urls() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix()
        .with_tool_dirs();
    context.temp_dir.child("wheels").create_dir_all()?;
    let lock = dependency(&context, "3.3")?;

    let mut redirected: toml::Value = toml::from_str(&lock)?;
    redirected["packages"][0]["wheels"][0]["url"] =
        toml::Value::String("https://example.invalid/idna-3.3-py3-none-any.whl".to_owned());
    // An index named in the lock must also be configured by the user.
    redirected["packages"][0]
        .as_table_mut()
        .context("Expected a package table")?
        .insert(
            "index".to_owned(),
            toml::Value::String("https://example.invalid/simple".to_owned()),
        );
    tool(
        &context,
        "1.0.0",
        Some(&toml::to_string(&redirected)?),
        None,
    )?;
    uv_snapshot!(context.filters(), context.tool_install()
        .args(["--locked", "--preview-features", "locked-tools", "--find-links", "wheels", "locked-tool==1.0.0"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The packaged lock for `locked-tool==1.0.0` contains unverified artifacts
      cause: Index `https://example.invalid/simple` from the packaged lock is not configured
    ");

    let mut wrong_path: toml::Value = toml::from_str(&lock)?;
    wrong_path["packages"][0]["wheels"][0]["url"] = toml::Value::String(
        "https://files.pythonhosted.org/packages/invalid/idna-3.3-py3-none-any.whl".to_owned(),
    );
    tool(
        &context,
        "2.0.0",
        Some(&toml::to_string(&wrong_path)?),
        None,
    )?;
    uv_snapshot!(context.filters(), context.tool_install()
        .args(["--locked", "--preview-features", "locked-tools", "--find-links", "wheels", "locked-tool==2.0.0"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The packaged lock for `locked-tool==2.0.0` contains unverified artifacts
      cause: URL for `idna==3.3` is not listed by https://pypi.org/simple: https://files.pythonhosted.org/packages/invalid/idna-3.3-py3-none-any.whl
    ");

    let mut unselected: toml::Value = toml::from_str(&lock)?;
    unselected["packages"][0]["sdist"]["url"] =
        toml::Value::String("https://example.invalid/idna-3.3.tar.gz".to_owned());
    tool(
        &context,
        "3.0.0",
        Some(&toml::to_string(&unselected)?),
        None,
    )?;
    uv_snapshot!(context.filters(), context.tool_install()
        .args(["--locked", "--preview-features", "locked-tools", "--find-links", "wheels", "locked-tool==3.0.0"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The packaged lock for `locked-tool==3.0.0` contains unverified artifacts
      cause: URL for `idna==3.3` is not listed by https://pypi.org/simple: https://example.invalid/idna-3.3.tar.gz
    ");

    let newer_lock: toml::Value = toml::from_str(&dependency(&context, "3.4")?)?;
    let mut wrong_version: toml::Value = toml::from_str(&lock)?;
    wrong_version["packages"][0]["sdist"]["url"] =
        newer_lock["packages"][0]["sdist"]["url"].clone();
    tool(
        &context,
        "4.0.0",
        Some(&toml::to_string(&wrong_version)?),
        None,
    )?;
    uv_snapshot!(context.filters(), context.tool_install()
        .args(["--locked", "--preview-features", "locked-tools", "--find-links", "wheels", "locked-tool==4.0.0"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The packaged lock for `locked-tool==4.0.0` contains unverified artifacts
      cause: URL for `idna==3.3` is not listed by https://pypi.org/simple: https://files.pythonhosted.org/packages/8b/e1/43beb3d38dba6cb420cefa297822eac205a277ab43e5ba5d5c46faf96438/idna-3.4.tar.gz
    ");

    tool(&context, "5.0.0", Some(&lock), None)?;
    uv_snapshot!(context.filters(), context.tool_install()
        .args(["--locked", "--preview-features", "locked-tools", "--no-index", "--find-links", "wheels", "locked-tool==5.0.0"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The packaged lock for `locked-tool==5.0.0` contains unverified artifacts
      cause: Index `https://pypi.org/simple` from the packaged lock is not configured
    ");
    Ok(())
}

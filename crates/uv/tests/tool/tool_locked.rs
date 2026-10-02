use std::collections::BTreeMap;
use std::fmt::Write;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use anyhow::{Context, Result};
#[cfg(feature = "test-pypi")]
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::FileWriteStr;
use assert_fs::fixture::{FileWriteBin, PathChild, PathCreateDir};
use indoc::{formatdoc, indoc};
use insta::allow_duplicates;
#[cfg(feature = "test-pypi")]
use insta::assert_snapshot;
use serde_json::json;
use sha2::{Digest, Sha256, Sha384, Sha512};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

use uv_extract::hash::Hasher;
use uv_pypi_types::{HashAlgorithm, HashDigest};
use uv_static::EnvVars;
use uv_test::archive::generate_source_archive;
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
async fn packaged_lock_overrides_and_excludes() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix()
        .with_tool_dirs();
    context.temp_dir.child("wheels").create_dir_all()?;
    let server = MockServer::start().await;
    let index = format!("{}/simple", server.uri());
    let mut index_files = BTreeMap::<String, Vec<serde_json::Value>>::new();
    let mut locked_files = BTreeMap::new();
    let mut override_hash = None;
    for (name, version, dependencies) in [
        ("idna", "3.3", vec!["beta==2.0"]),
        ("idna", "3.5", vec!["beta[feature]", "gamma==1.0"]),
        ("beta", "1.0", vec![]),
        ("delta", "1.0", vec![]),
        ("gamma", "1.0", vec![]),
    ] {
        let dependencies = dependencies
            .into_iter()
            .map(str::parse)
            .collect::<Result<Vec<_>, _>>()?;
        let extras = if name == "beta" {
            BTreeMap::from([("feature".parse()?, vec!["delta==1.0".parse()?])])
        } else {
            BTreeMap::new()
        };
        let (filename, wheel) = generate_wheel_with_files(
            &name.parse()?,
            &version.parse()?,
            &dependencies,
            &extras,
            None,
            "py3-none-any",
            &[],
        );
        let hash = hex::encode(Sha256::digest(&wheel));
        let url = format!("{}/files/{filename}", server.uri());
        index_files
            .entry(name.to_string())
            .or_default()
            .push(json!({
                "filename": filename,
                "url": url,
                "hashes": { "sha256": hash },
                "upload-time": "2023-01-01T00:00:00Z",
            }));
        Mock::given(method("GET"))
            .and(path(format!("/files/{filename}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(wheel))
            .mount(&server)
            .await;
        if (name, version) == ("idna", "3.3") || (name, version) == ("beta", "1.0") {
            locked_files.insert(name, (version, url, hash));
        } else if (name, version) == ("idna", "3.5") {
            override_hash = Some(hash);
        }
    }
    for (name, files) in index_files {
        Mock::given(method("GET"))
            .and(path(format!("/simple/{name}/")))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                json!({"meta": {"api-version": "1.1"}, "name": name, "files": files}).to_string(),
                "application/vnd.pypi.simple.v1+json",
            ))
            .mount(&server)
            .await;
    }
    let mut lock = String::from("lock-version = \"1.0\"\ncreated-by = \"test\"\n");
    for (name, (version, url, hash)) in locked_files {
        writeln!(
            lock,
            "[[packages]]\nname = \"{name}\"\nversion = \"{version}\"\nindex = \"{index}\"\nwheels = [{{ url = \"{url}\", hashes = {{ sha256 = \"{hash}\" }} }}]"
        )?;
    }
    tool(&context, "1.0.0", Some(&lock), None)?;
    let override_hash = override_hash.context("missing override hash")?;
    let context = context.with_filter((override_hash.clone(), "[OVERRIDE_HASH]"));
    context
        .temp_dir
        .child("overrides.txt")
        .write_str(&format!("idna==3.5 --hash=sha256:{override_hash}\n"))?;
    context.temp_dir.child("bad-override.txt").write_str(
        "idna==3.5 --hash=sha256:0000000000000000000000000000000000000000000000000000000000000000\n",
    )?;
    context.temp_dir.child("excludes.txt").write_str("idna\n")?;
    context.temp_dir.child("invalid-inactive-override.txt").write_str(&format!(
        "idna @ {}/files/idna-3.5-py3-none-any.whl#sha256={override_hash}&sha512=bad ; sys_platform == 'never'\n",
        server.uri()
    ))?;
    let output = context
        .tool_install()
        .args([
            "--locked",
            "--preview-features",
            "locked-tools",
            "--index-url",
            &index,
            "--find-links",
            "wheels",
            "--override",
            "invalid-inactive-override.txt",
            "locked-tool",
        ])
        .env(EnvVars::PATH, context.temp_dir.child("bin").path())
        .output()?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Invalid hash digest length"));
    assert!(
        !context
            .temp_dir
            .child("tools/locked-tool/uv-receipt.toml")
            .path()
            .exists()
    );
    context
        .temp_dir
        .child("unused-exclude.txt")
        .write_str("unused-package\n")?;

    uv_snapshot!(context.filters(), context.tool_run().args(["--locked", "--preview-features", "locked-tools", "--index-url", &index, "--find-links", "wheels", "--override", "overrides.txt", "locked-tool"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    3.5

    ----- stderr -----
    Installed [N] packages in [TIME]
     + beta==1.0
     + delta==1.0
     + gamma==1.0
     + idna==3.5
     + locked-tool==1.0.0
    ");

    let scoped = context.temp_dir.child("scoped.py");
    let scoped_script = indoc! {r#"
        # /// script
        # dependencies = []
        # [tool.uv]
        # override-dependencies = [{ package = { name = "locked-tool", version = "1.0.0" }, dependencies = ["idna==3.5"] }]
        # ///
    "#};
    scoped.write_str(scoped_script)?;
    uv_snapshot!(context.filters(), context.tool_run().args(["--locked", "--isolated", "--preview-features", "locked-tools", "--index-url", &index, "--find-links", "wheels", "--override", "-", "locked-tool"]).stdin(fs_err::File::open(scoped.path())?.into_file()), @"
    exit_code: 0 (success)
    ----- stdout -----
    3.5
    ");
    scoped.write_str(&scoped_script.replace("version = \"1.0.0\"", "version = \"2.0.0\""))?;
    uv_snapshot!(context.filters(), context.tool_run().args(["--locked", "--isolated", "--preview-features", "locked-tools", "--index-url", &index, "--find-links", "wheels", "--override", "-", "locked-tool"]).stdin(fs_err::File::open(scoped.path())?.into_file()), @"
    exit_code: 0 (success)
    ----- stdout -----
    3.3

    ----- stderr -----
    Installed [N] packages in [TIME]
     + beta==1.0
     + idna==3.3
     + locked-tool==1.0.0
    ");

    scoped.write_str(indoc! {r#"
        # /// script
        # dependencies = []
        # [tool.uv]
        # override-dependencies = [{ package = { name = "idna", version = "3.3" }, dependencies = ["gamma==1.0"] }]
        # ///
    "#})?;
    uv_snapshot!(context.filters(), context.tool_run().args(["--locked", "--isolated", "--preview-features", "locked-tools", "--index-url", &index, "--find-links", "wheels", "--override", "-", "locked-tool"]).stdin(fs_err::File::open(scoped.path())?.into_file()), @"
    exit_code: 0 (success)
    ----- stdout -----
    3.3

    ----- stderr -----
    Installed [N] packages in [TIME]
     + beta==1.0
     + gamma==1.0
     + idna==3.3
     + locked-tool==1.0.0
    ");

    uv_snapshot!(context.filters(), context.tool_run().args(["--locked", "--preview-features", "locked-tools", "--index-url", &index, "--find-links", "wheels", "--override", "bad-override.txt", "locked-tool"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to download `idna==3.5`
      cause: Hash mismatch for `idna==3.5`

             Expected:
               sha256:0000000000000000000000000000000000000000000000000000000000000000
               sha256:[OVERRIDE_HASH]

             Computed:
               sha256:[OVERRIDE_HASH]
    ");

    let previous = server
        .received_requests()
        .await
        .context("request recording disabled")?
        .len();
    uv_snapshot!(context.filters(), context.tool_install().args(["--locked", "--preview-features", "locked-tools", "--index-url", &index, "--find-links", "wheels", "--exclude", "excludes.txt", "locked-tool"]).env(EnvVars::PATH, context.temp_dir.child("bin").path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed [N] packages in [TIME]
     + beta==1.0
     + locked-tool==1.0.0
    Installed 1 executable: locked-tool
    ");
    let requests = server
        .received_requests()
        .await
        .context("request recording disabled")?;
    assert!(
        !requests[previous..]
            .iter()
            .any(|request| request.url.path().contains("/files/idna-3.3"))
    );
    uv_snapshot!(context.filters(), context.tool_install().args(["--locked", "--preview-features", "locked-tools", "--index-url", &index, "--find-links", "wheels", "--exclude", "unused-exclude.txt", "--force", "locked-tool"]).env(EnvVars::PATH, context.temp_dir.child("bin").path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed [N] packages in [TIME]
     + beta==1.0
     + idna==3.3
     + locked-tool==1.0.0
    Installed 1 executable: locked-tool
    ");
    uv_snapshot!(context.filters(), context.tool_install().args(["--locked", "--preview-features", "locked-tools", "--index-url", &index, "--find-links", "wheels", "--override", "overrides.txt", "--force", "locked-tool"]).env(EnvVars::PATH, context.temp_dir.child("bin").path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed [N] packages in [TIME]
     + beta==1.0
     + delta==1.0
     + gamma==1.0
     + idna==3.5
     + locked-tool==1.0.0
    Installed 1 executable: locked-tool
    ");

    uv_snapshot!(context.filters(), context.tool_install().args(["--preview-features", "locked-tools,tool-install-locks", "--index-url", &index, "--find-links", "wheels", "--override", "overrides.txt", "locked-tool"]).env(EnvVars::PATH, context.temp_dir.child("bin").path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    `locked-tool` is already installed
    ");
    uv_snapshot!(context.filters(), context.tool_install().args(["--preview-features", "locked-tools", "--index-url", &index, "--find-links", "wheels", "--override", "bad-override.txt", "locked-tool"]).env(EnvVars::PATH, context.temp_dir.child("bin").path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved [N] packages in [TIME]
    Checked [N] packages in [TIME]
    Installed 1 executable: locked-tool
    ");
    let receipt = context.read("tools/locked-tool/uv-receipt.toml");
    assert!(receipt.contains(&"0".repeat(64)));
    assert!(!receipt.contains("locked = true"));
    context
        .tool_install()
        .args([
            "--locked",
            "--preview-features",
            "locked-tools",
            "--index-url",
            &index,
            "--find-links",
            "wheels",
            "--override",
            "overrides.txt",
            "--force",
            "locked-tool",
        ])
        .env(EnvVars::PATH, context.temp_dir.child("bin").path())
        .assert()
        .success();
    let receipt = context.read("tools/locked-tool/uv-receipt.toml");
    assert!(receipt.contains(&override_hash));
    assert!(receipt.contains("locked = true"));
    context
        .temp_dir
        .child("tools/locked-tool/uv-receipt.toml")
        .write_str(&receipt.replace(
            &override_hash,
            "0000000000000000000000000000000000000000000000000000000000000000",
        ))?;
    uv_snapshot!(context.filters(), context.tool_upgrade().args(["--preview-features", "locked-tools", "--index-url", &index, "--find-links", "wheels", "locked-tool"]).env(EnvVars::PATH, context.temp_dir.child("bin").path()), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: Failed to upgrade locked-tool
      cause: Failed to download `idna==3.5`
      cause: Hash mismatch for `idna==3.5`

             Expected:
               sha256:0000000000000000000000000000000000000000000000000000000000000000
               sha256:[OVERRIDE_HASH]

             Computed:
               sha256:[OVERRIDE_HASH]
    ");
    let script = context.temp_dir.child("overrides.py");
    script.write_str(indoc! {r#"
        # /// script
        # dependencies = []
        # [tool.uv]
        # override-dependencies = ["idna==3.5"]
        # ///
    "#})?;
    context
        .tool_install()
        .args([
            "--locked",
            "--preview-features",
            "locked-tools",
            "--index-url",
            &index,
            "--find-links",
            "wheels",
            "--override",
            "-",
            "--force",
            "locked-tool",
        ])
        .stdin(fs_err::File::open(script.path())?.into_file())
        .env(EnvVars::PATH, context.temp_dir.child("bin").path())
        .assert()
        .success();
    let receipt = context.read("tools/locked-tool/uv-receipt.toml");
    let receipt: toml::Value = toml::from_str(&receipt)?;
    let overrides = receipt["tool"]["overrides"]
        .as_array()
        .context("missing overrides")?;
    assert!(
        overrides
            .iter()
            .any(|entry| entry["name"].as_str() == Some("idna")
                && entry["specifier"].as_str() == Some("==3.5"))
    );
    uv_snapshot!(context.filters(), context.tool_install().args(["--preview-features", "locked-tools", "--index-url", &index, "--find-links", "wheels", "--override", "-", "locked-tool"]).stdin(fs_err::File::open(script.path())?.into_file()).env(EnvVars::PATH, context.temp_dir.child("bin").path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    `locked-tool` is already installed
    ");
    assert!(
        context
            .read("tools/locked-tool/uv-receipt.toml")
            .contains("locked = true")
    );
    uv_snapshot!(context.filters(), context.tool_upgrade().args(["--preview-features", "locked-tools", "--index-url", &index, "--find-links", "wheels", "locked-tool"]).env(EnvVars::PATH, context.temp_dir.child("bin").path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Modified locked-tool environment
     ~ beta==1.0
     ~ delta==1.0
     ~ gamma==1.0
     ~ idna==3.5
     ~ locked-tool==1.0.0
    Installed 1 executable: locked-tool
    ");

    scoped.write_str(scoped_script)?;
    context
        .tool_install()
        .args([
            "--locked",
            "--preview-features",
            "locked-tools",
            "--index-url",
            &index,
            "--find-links",
            "wheels",
            "--override",
            "-",
            "--force",
            "locked-tool",
        ])
        .stdin(fs_err::File::open(scoped.path())?.into_file())
        .env(EnvVars::PATH, context.temp_dir.child("bin").path())
        .assert()
        .success();
    let receipt: toml::Value = toml::from_str(&context.read("tools/locked-tool/uv-receipt.toml"))?;
    assert_eq!(
        receipt["tool"]["scoped-overrides"].as_array().map(Vec::len),
        Some(1)
    );
    context
        .tool_upgrade()
        .args([
            "--preview-features",
            "locked-tools",
            "--index-url",
            &index,
            "--find-links",
            "wheels",
            "locked-tool",
        ])
        .env(EnvVars::PATH, context.temp_dir.child("bin").path())
        .assert()
        .success();
    let executable = context
        .temp_dir
        .child("bin")
        .child(format!("locked-tool{}", std::env::consts::EXE_SUFFIX));
    let output = std::process::Command::new(executable.path()).output()?;
    assert!(output.status.success());
    assert_eq!(std::str::from_utf8(&output.stdout)?.trim_end(), "3.5");

    let grouped_lock = lock
        .replace(
            "created-by = \"test\"",
            "created-by = \"test\"\ndependency-groups = [\"tools\"]\ndefault-groups = [\"tools\"]",
        )
        .replace(
            "[[packages]]\nname = \"beta\"",
            "[[packages]]\nmarker = \"'tools' in dependency_groups\"\nname = \"beta\"",
        );
    tool(&context, "1.1.0", Some(&grouped_lock), None)?;
    uv_snapshot!(context.filters(), context.tool_run().args(["--locked", "--isolated", "--preview-features", "locked-tools", "--index-url", &index, "--find-links", "wheels", "--override", "overrides.txt", "locked-tool==1.1.0"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    3.5

    ----- stderr -----
    Installed [N] packages in [TIME]
     + beta==1.0
     + delta==1.0
     + gamma==1.0
     + idna==3.5
     + locked-tool==1.1.0
    ");
    Ok(())
}

#[tokio::test]
async fn packaged_lock_removed_packages_do_not_require_old_indexes() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_tool_dirs();
    context.temp_dir.child("wheels").create_dir_all()?;
    let index = "https://unconfigured.example/simple";
    let mut lock = locked_artifact(
        index,
        "https://unconfigured.example/idna-3.3-py3-none-any.whl",
        "idna-3.3-py3-none-any.whl",
        &format!("sha256 = \"{}\"", "0".repeat(64)),
        "wheels",
    );
    writeln!(
        lock,
        "[[packages]]\nname = \"beta\"\nversion = \"1.0\"\nindex = \"{index}\"\nwheels = [{{ url = \"https://unconfigured.example/beta-1.0-py3-none-any.whl\", hashes = {{ sha256 = \"{}\" }} }}]",
        "0".repeat(64)
    )?;
    tool(&context, "1.0.0", Some(&lock), None)?;
    let mut overrides = String::new();
    let mut constraint = String::new();
    let mut idna_hash = None;
    let mut beta_hash = None;
    for (name, version) in [("idna", "3.5"), ("beta", "2.0")] {
        let (filename, wheel) = generate_wheel_with_files(
            &name.parse()?,
            &version.parse()?,
            &[],
            &BTreeMap::new(),
            None,
            "py3-none-any",
            &[],
        );
        let path = context.temp_dir.child("wheels").child(filename);
        path.write_binary(&wheel)?;
        let url = url::Url::from_file_path(path.path())
            .map_err(|()| anyhow::anyhow!("Could not create a file URL"))?;
        let hash = hex::encode(Sha256::digest(&wheel));
        writeln!(overrides, "{name} @ {url}#sha256={hash}")?;
        if name == "idna" {
            writeln!(constraint, "{name} @ {url}#sha256={hash}")?;
            idna_hash = Some(hash);
        } else {
            beta_hash = Some(hash);
        }
    }
    let beta_hash = beta_hash.context("missing beta hash")?;
    let idna_hash = idna_hash.context("missing idna hash")?;
    let context = context
        .with_filter((beta_hash, "[BETA_HASH]"))
        .with_filter((idna_hash.clone(), "[IDNA_HASH]"));
    context
        .temp_dir
        .child("overrides.txt")
        .write_str(&overrides)?;
    context
        .temp_dir
        .child("constraints.txt")
        .write_str(&constraint)?;
    context
        .temp_dir
        .child("excludes.txt")
        .write_str("idna\nbeta\n")?;

    uv_snapshot!(context.filters(), context.tool_install().args(["--locked", "--preview-features", "locked-tools", "--find-links", "wheels", "--override", "overrides.txt", "--constraint", "constraints.txt", "locked-tool"]).env(EnvVars::PATH, context.temp_dir.child("bin").path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed [N] packages in [TIME]
     + beta==2.0 (from file://[TEMP_DIR]/wheels/beta-2.0-py3-none-any.whl#sha256=[BETA_HASH])
     + idna==3.5 (from file://[TEMP_DIR]/wheels/idna-3.5-py3-none-any.whl#sha256=[IDNA_HASH])
     + locked-tool==1.0.0
    Installed 1 executable: locked-tool
    ");
    let receipt = context.read("tools/locked-tool/uv-receipt.toml");
    assert!(receipt.contains(&idna_hash));
    context
        .temp_dir
        .child("tools/locked-tool/uv-receipt.toml")
        .write_str(&receipt.replace(&idna_hash, &"0".repeat(64)))?;
    let output = context
        .tool_upgrade()
        .args([
            "--preview-features",
            "locked-tools",
            "--find-links",
            "wheels",
            "locked-tool",
        ])
        .env(EnvVars::PATH, context.temp_dir.child("bin").path())
        .output()?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Hash mismatch"));
    uv_snapshot!(context.filters(), context.tool_install().args(["--locked", "--preview-features", "locked-tools", "--find-links", "wheels", "--exclude", "excludes.txt", "--force", "locked-tool"]).env(EnvVars::PATH, context.temp_dir.child("bin").path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed [N] packages in [TIME]
     + locked-tool==1.0.0
    Installed 1 executable: locked-tool
    ");
    Ok(())
}

#[tokio::test]
async fn packaged_lock_preserves_tool_url_hash_on_upgrade() -> Result<()> {
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
    let output = context
        .tool_install()
        .args([
            "--preview-features",
            "locked-tools",
            "--from",
            &requirement,
            "locked-tool",
        ])
        .env(EnvVars::PATH, context.temp_dir.child("bin").path())
        .output()?;
    assert!(output.status.success());
    let receipt = context.read("tools/locked-tool/uv-receipt.toml");
    assert!(
        receipt.contains("locked = true"),
        "{}\n{receipt}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(receipt.contains(&hash));
    context
        .tool_upgrade()
        .args([
            "--unlocked",
            "--preview-features",
            "locked-tools",
            "locked-tool",
        ])
        .env(EnvVars::PATH, context.temp_dir.child("bin").path())
        .assert()
        .success();
    let receipt = context.read("tools/locked-tool/uv-receipt.toml");
    assert!(!receipt.contains("locked = true"));
    assert!(receipt.contains(&hash));
    context
        .temp_dir
        .child("tools/locked-tool/uv-receipt.toml")
        .write_str(&receipt.replace(&hash, &"0".repeat(64)))?;
    let output = context
        .tool_upgrade()
        .args([
            "--locked",
            "--preview-features",
            "locked-tools",
            "locked-tool",
        ])
        .env(EnvVars::PATH, context.temp_dir.child("bin").path())
        .output()?;
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("does not match the required hashes"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[tokio::test]
async fn packaged_lock_override_cannot_activate_tool_extra() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_tool_dirs();
    context.temp_dir.child("wheels").create_dir_all()?;
    let server = MockServer::start().await;
    let index = format!("{}/simple", server.uri());
    let (filename, bytes) = generate_wheel_with_files(
        &"epsilon".parse()?,
        &"1.0".parse()?,
        &[],
        &BTreeMap::new(),
        None,
        "py3-none-any",
        &[],
    );
    let hash = hex::encode(Sha256::digest(&bytes));
    Mock::given(method("GET"))
        .and(path("/simple/epsilon/"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(
                json!({"meta": {"api-version": "1.1"}, "name": "epsilon", "files": [{
                    "filename": filename,
                "url": format!("{}/files/{filename}", server.uri()),
                "hashes": {"sha256": hash},
                "upload-time": "2023-01-01T00:00:00Z",
                }]})
                .to_string(),
                "application/vnd.pypi.simple.v1+json",
            ),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/files/{filename}")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes))
        .mount(&server)
        .await;
    let (filename, bytes) = generate_wheel_with_files(
        &"idna".parse()?,
        &"3.5".parse()?,
        &["locked-tool[feature]".parse()?],
        &BTreeMap::new(),
        None,
        "py3-none-any",
        &[],
    );
    let override_wheel = context.temp_dir.child("wheels").child(filename);
    override_wheel.write_binary(&bytes)?;
    let override_url = url::Url::from_file_path(override_wheel.path())
        .map_err(|()| anyhow::anyhow!("Could not create a file URL"))?;
    context
        .temp_dir
        .child("overrides.txt")
        .write_str(&format!("idna @ {override_url}\n"))?;
    let lock = locked_artifact(
        "https://unconfigured.example/simple",
        "https://unconfigured.example/idna-3.3-py3-none-any.whl",
        "idna-3.3-py3-none-any.whl",
        &format!("sha256 = \"{}\"", "0".repeat(64)),
        "wheels",
    );
    let (filename, bytes) = generate_wheel_with_files(
        &"locked-tool".parse()?,
        &"1.0.0".parse()?,
        &["idna>=3.3,<3.5".parse()?],
        &BTreeMap::from([("feature".parse()?, vec!["epsilon==1.0".parse()?])]),
        None,
        "py3-none-any",
        &[
            ("locked_tool-1.0.0.dist-info/pylock.toml", &lock),
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
    uv_snapshot!(context.filters(), context.tool_install().args(["--locked", "--preview-features", "locked-tools", "--index-url", &index, "--find-links", "wheels", "--override", "overrides.txt", "locked-tool"]).env(EnvVars::PATH, context.temp_dir.child("bin").path()), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: An override requires the extra `feature` of locked package `locked-tool`, but extras are not supported with `--locked`
    ");
    Ok(())
}

#[tokio::test]
async fn packaged_lock_refines_python_for_tool() -> Result<()> {
    let context = uv_test::test_context_with_versions!(&["3.11", "3.12", "3.13"])
        .with_filtered_counts()
        .with_tool_dirs();
    let server = MockServer::start().await;
    let index = format!("{}/simple", server.uri());
    let mut files = Vec::new();
    for (version, lock_python) in [("1.0.0", ">=3.12"), ("2.0.0", ">=3.13")] {
        let lock = format!(
            "lock-version = \"1.0\"\ncreated-by = \"test\"\nrequires-python = \"{lock_python}\"\npackages = []\n"
        );
        let metadata_path = format!("locked_tool-{version}.dist-info");
        let (filename, wheel) = generate_wheel_with_files(
            &"locked-tool".parse()?,
            &version.parse()?,
            &[],
            &BTreeMap::new(),
            Some(&">=3.12".parse()?),
            "py3-none-any",
            &[
                (
                    "locked_tool/cli.py",
                    "import sys\ndef main(): print('.'.join(str(x) for x in sys.version_info[:2]))\n",
                ),
                (
                    &format!("{metadata_path}/entry_points.txt"),
                    "[console_scripts]\nlocked-tool = locked_tool.cli:main\n",
                ),
                (&format!("{metadata_path}/pylock.toml"), &lock),
            ],
        );
        let url = format!("{}/files/{filename}", server.uri());
        files.push(json!({
            "filename": filename,
            "url": url,
            "hashes": { "sha256": hex::encode(Sha256::digest(&wheel)) },
            "requires-python": ">=3.12",
            "upload-time": "2023-01-01T00:00:00Z",
        }));
        Mock::given(method("GET"))
            .and(path(format!("/files/{filename}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(wheel))
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/simple/locked-tool/"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(
                json!({"meta": {"api-version": "1.1"}, "name": "locked-tool", "files": files})
                    .to_string(),
                "application/vnd.pypi.simple.v1+json",
            ),
        )
        .mount(&server)
        .await;

    uv_snapshot!(context.filters(), context.tool_run().args(["--locked", "--preview-features", "locked-tools", "--index-url", &index, "--from", "locked-tool==1.0.0", "locked-tool"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    3.12

    ----- stderr -----
    Installed [N] packages in [TIME]
     + locked-tool==1.0.0
    ");
    uv_snapshot!(context.filters(), context.tool_install().args(["--locked", "--preview-features", "locked-tools", "--index-url", &index, "--python", "3.11", "locked-tool==1.0.0"]), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies
      cause: Because the current Python version (3.11.[X]) does not satisfy Python>=3.12 and locked-tool==1.0.0 depends on Python>=3.12, we can conclude that locked-tool==1.0.0 cannot be used.
             And because only locked-tool>=1.0.0 is available and you require locked-tool==1.0.0, we can conclude that your requirements are unsatisfiable.
    ");
    uv_snapshot!(context.filters(), context.tool_install().args(["--locked", "--preview-features", "locked-tools", "--index-url", &index, "locked-tool==1.0.0"]).env(EnvVars::PATH, context.temp_dir.child("bin").path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed [N] packages in [TIME]
     + locked-tool==1.0.0
    Installed 1 executable: locked-tool
    ");
    uv_snapshot!(context.filters(), context.tool_run().args(["--locked", "--preview-features", "locked-tools", "--index-url", &index, "--python", "3.11", "--from", "locked-tool==1.0.0", "locked-tool"]), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving tool dependencies
      cause: Because the current Python version (3.11.[X]) does not satisfy Python>=3.12 and locked-tool==1.0.0 depends on Python>=3.12, we can conclude that locked-tool==1.0.0 cannot be used.
             And because only locked-tool>=1.0.0 is available and you require locked-tool==1.0.0, we can conclude that your requirements are unsatisfiable.
    ");
    uv_snapshot!(context.filters(), context.tool_run().args(["--locked", "--preview-features", "locked-tools", "--index-url", &index, "--python", ">=3.11", "--from", "locked-tool==1.0.0", "locked-tool"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    3.12
    ");
    uv_snapshot!(context.filters(), context.tool_run().args(["--locked", "--preview-features", "locked-tools", "--index-url", &index, "--from", "locked-tool==2.0.0", "locked-tool"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The requested interpreter resolved to Python 3.12.[X], which is incompatible with the `pylock.toml`'s Python requirement: `>=3.13`
    ");
    Ok(())
}

#[test]
fn packaged_lock_rejects_sources_before_build() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_tool_dirs();
    context.temp_dir.child("source").create_dir_all()?;
    context
        .temp_dir
        .child("source/pyproject.toml")
        .write_str(indoc! {r#"
        [build-system]
        requires = []
        build-backend = "backend"
        backend-path = ["."]
    "#})?;
    context
        .temp_dir
        .child("source/backend.py")
        .write_str(&format!(
            "from pathlib import Path\nPath({:?}).write_text('executed')\n",
            context.temp_dir.child("executed").path().to_string_lossy()
        ))?;
    uv_snapshot!(context.filters(), context.tool_run().args(["--locked", "--preview-features", "locked-tools", "--no-index", "--from", "./source", "source-tool"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: A wheel is required to install a tool with `--locked`
    ");
    uv_snapshot!(context.filters(), context.tool_install().args(["--locked", "--preview-features", "locked-tools", "--no-index", "./source"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: A wheel is required to install a tool with `--locked`
    ");
    uv_snapshot!(context.filters(), context.tool_run().args(["--locked", "--preview-features", "locked-tools", "--no-index", "--with", "./source", "locked-tool"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: `--locked` requires a single tool package and cannot be combined with `--with`
    ");
    uv_snapshot!(context.filters(), context.tool_install().args(["--locked", "--preview-features", "locked-tools", "--no-index", "--with", "./source", "locked-tool"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: `--locked` requires a single tool package and cannot be combined with `--with`
    ");
    assert!(!context.temp_dir.child("executed").path().exists());
    Ok(())
}

#[tokio::test]
async fn packaged_lock_root_url_hash() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_tool_dirs();
    context.temp_dir.child("wheels").create_dir_all()?;
    tool(&context, "1.0.0", None, None)?;
    let wheel = context
        .temp_dir
        .child("wheels/locked_tool-1.0.0-py3-none-any.whl");
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/locked_tool-1.0.0-py3-none-any.whl"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(fs_err::read(wheel.path())?))
        .mount(&server)
        .await;
    let url = format!(
        "{}/locked_tool-1.0.0-py3-none-any.whl#sha256={}",
        server.uri(),
        "0".repeat(64)
    );
    uv_snapshot!(context.filters(), context.tool_run().args(["--locked", "--preview-features", "locked-tools", "--no-index", "--from", &url, "locked-tool"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The selected artifact for `locked-tool @ http://[LOCALHOST]/locked_tool-1.0.0-py3-none-any.whl#sha256=0000000000000000000000000000000000000000000000000000000000000000` does not match the required hashes
    ");
    Ok(())
}

#[tokio::test]
async fn packaged_lock_root_direct_hashes() -> Result<()> {
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
    let wheel = context.temp_dir.child("wheels").child(filename);
    wheel.write_binary(&bytes)?;
    let url = url::Url::from_file_path(wheel.path())
        .map_err(|()| anyhow::anyhow!("Could not create a file URL"))?;
    let hash = hex::encode(Sha256::digest(&bytes));
    let requirement = format!(
        "locked-tool @ {url} --hash=sha256:{hash} --hash=sha512:{}\n",
        "0".repeat(128)
    );
    let input = context.temp_dir.child("hashes.txt");
    input.write_str(&requirement)?;
    let from = format!("locked-tool @ {url}");
    let mut outputs = Vec::new();
    for option in ["--constraint", "--override"] {
        let output = context
            .tool_install()
            .args([
                "--locked",
                "--preview-features",
                "locked-tools",
                "--no-index",
                option,
                "hashes.txt",
                "--from",
                &from,
                "locked-tool",
            ])
            .env(EnvVars::PATH, context.temp_dir.child("bin").path())
            .output()?;
        outputs.push((option, output));
    }
    assert!(
        outputs.iter().all(|(_, output)| {
            let stderr = String::from_utf8_lossy(&output.stderr);
            !output.status.success()
                && (stderr.contains("Hash mismatch")
                    || stderr.contains("does not match the required hashes"))
        }),
        "{}",
        outputs
            .iter()
            .map(|(option, output)| format!(
                "{option}: status {}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    Ok(())
}

#[tokio::test]
async fn packaged_lock_binds_hashless_tool_wheel() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_tool_dirs();
    let server = MockServer::start().await;
    let name = "race-tool".parse()?;
    let version = "1.0.0".parse()?;
    let make_wheel = |message: &str, lock: &str| {
        generate_wheel_with_files(
            &name,
            &version,
            &[],
            &BTreeMap::new(),
            None,
            "py3-none-any",
            &[
                (
                    "race_tool/cli.py",
                    &format!("def main(): print({message:?})\n"),
                ),
                (
                    "race_tool-1.0.0.dist-info/entry_points.txt",
                    "[console_scripts]\nrace-tool = race_tool.cli:main\n",
                ),
                ("race_tool-1.0.0.dist-info/pylock.toml", lock),
            ],
        )
    };
    let (filename, first) = make_wheel(
        "first",
        "lock-version = \"1.0\"\ncreated-by = \"test\"\npackages = []\n",
    );
    let (_, second) = make_wheel("second", "invalid lock");
    let context = context
        .with_filter((hex::encode(Sha256::digest(&first)), "[FIRST_HASH]"))
        .with_filter((hex::encode(Sha256::digest(&second)), "[SECOND_HASH]"));
    let index = format!("{}/simple", server.uri());
    let url = format!("{}/files/{filename}", server.uri());
    Mock::given(method("GET"))
        .and(path("/simple/race-tool/"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("cache-control", "no-store")
                .set_body_raw(
                    json!({
                        "meta": { "api-version": "1.1" },
                        "name": "race-tool",
                        "files": [
                            { "filename": filename, "url": url, "hashes": {}, "upload-time": "2023-01-01T00:00:00Z" },
                            { "filename": "race_tool-2.0.0.tar.gz", "url": format!("{}/files/race_tool-2.0.0.tar.gz", server.uri()), "hashes": {}, "upload-time": "2023-01-01T00:00:00Z" }
                        ]
                    })
                    .to_string(),
                    "application/vnd.pypi.simple.v1+json",
                ),
        )
        .mount(&server)
        .await;
    let requests = Arc::new(AtomicUsize::new(0));
    Mock::given(method("GET"))
        .and(path(format!("/files/{filename}")))
        .respond_with(move |_: &wiremock::Request| {
            let body = if requests.fetch_add(1, Ordering::SeqCst) == 0 {
                &first
            } else {
                &second
            };
            ResponseTemplate::new(200)
                .insert_header("cache-control", "no-store")
                .set_body_bytes(body.clone())
        })
        .mount(&server)
        .await;
    uv_snapshot!(context.filters(), context.tool_run()
        .args([
            "--locked",
            "--preview-features",
            "locked-tools",
            "--refresh",
            "--index-url",
            &index,
            "race-tool",
        ]), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: Failed to download `race-tool==1.0.0`
      cause: Hash mismatch for `race-tool==1.0.0`

             Expected:
               sha256:[FIRST_HASH]

             Computed:
               sha256:[SECOND_HASH]
    ");
    Ok(())
}

#[tokio::test]
async fn packaged_lock_respects_index_priority() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_tool_dirs();
    context.temp_dir.child("wheels").create_dir_all()?;
    let server = MockServer::start().await;
    let first_index = format!("{}/first", server.uri());
    let second_index = format!("{}/second", server.uri());
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
    for (index, prefix) in [(&first_index, "first"), (&second_index, "second")] {
        let url = format!("{index}/files/{filename}");
        Mock::given(method("GET"))
            .and(path(format!("/{prefix}/idna/")))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                json!({
                    "meta": { "api-version": "1.1" },
                    "name": "idna",
                    "files": [{ "filename": filename, "url": url, "hashes": { "sha256": hash } }]
                })
                .to_string(),
                "application/vnd.pypi.simple.v1+json",
            ))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/{prefix}/files/{filename}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(wheel.clone()))
            .mount(&server)
            .await;
    }
    let lock = formatdoc! {r#"
        lock-version = "1.0"
        created-by = "test"
        [[packages]]
        name = "idna"
        version = "3.3"
        index = "{second_index}"
        wheels = [{{ url = "{second_index}/files/{filename}", hashes = {{ sha256 = "{hash}" }} }}]
    "#};
    tool(&context, "1.0.0", Some(&lock), None)?;
    // Cache the second index before introducing a higher-priority index.
    context
        .tool_run()
        .args([
            "--locked",
            "--preview-features",
            "locked-tools",
            "--find-links",
            "wheels",
            "--index-url",
            &second_index,
            "locked-tool",
        ])
        .assert()
        .success();
    uv_snapshot!(context.filters(), context.tool_run()
        .args([
            "--locked",
            "--preview-features",
            "locked-tools",
            "--find-links",
            "wheels",
            "--index",
            &first_index,
            "--default-index",
            &second_index,
            "locked-tool",
        ]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The packaged lock for `locked-tool==1.0.0` contains unverified artifacts
      cause: Index `http://[LOCALHOST]/second` from the packaged lock is not selected for `idna` by the configured index strategy
    ");

    uv_snapshot!(context.filters(), context.tool_run()
        .args([
            "--locked",
            "--preview-features",
            "locked-tools",
            "--find-links",
            "wheels",
            "--index",
            &first_index,
            "--default-index",
            &second_index,
            "--index-strategy",
            "unsafe-best-match",
            "locked-tool",
        ]), @"
    exit_code: 0 (success)
    ----- stdout -----
    3.3
    ");

    context
        .temp_dir
        .child("uv.toml")
        .write_str(&formatdoc! {r#"
        [[index]]
        name = "explicit"
        url = "{second_index}"
        explicit = true
        [[index]]
        url = "{first_index}"
        default = true
    "#})?;
    uv_snapshot!(context.filters(), context.tool_run()
        .args([
            "--locked",
            "--preview-features",
            "locked-tools",
            "--find-links",
            "wheels",
            "--config-file",
            "uv.toml",
            "locked-tool",
        ]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The packaged lock for `locked-tool==1.0.0` contains unverified artifacts
      cause: Index `http://[LOCALHOST]/second` from the packaged lock is not selected for `idna` by the configured index strategy
    ");
    Ok(())
}

#[tokio::test]
async fn packaged_lock_refresh_rechecks_index() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_tool_dirs();
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
    let requests = Arc::new(AtomicUsize::new(0));
    let index_requests = Arc::clone(&requests);
    let listed = json!({
        "meta": { "api-version": "1.1" },
        "name": "idna",
        "files": [{ "filename": filename, "url": url, "hashes": { "sha256": hash } }]
    });
    let removed = json!({ "meta": { "api-version": "1.1" }, "name": "idna", "files": [] });
    Mock::given(method("GET"))
        .and(path("/simple/idna/"))
        .respond_with(move |_: &wiremock::Request| {
            let body = if index_requests.fetch_add(1, Ordering::SeqCst) == 0 {
                &listed
            } else {
                &removed
            };
            ResponseTemplate::new(200)
                .insert_header("cache-control", "max-age=3600")
                .set_body_raw(body.to_string(), "application/vnd.pypi.simple.v1+json")
        })
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
    let args = [
        "--locked",
        "--isolated",
        "--preview-features",
        "locked-tools",
        "--find-links",
        "wheels",
        "--index-url",
        &index,
    ];
    context
        .tool_run()
        .args(args)
        .arg("locked-tool")
        .assert()
        .success();
    context
        .tool_run()
        .args(args)
        .arg("locked-tool")
        .assert()
        .success();
    assert_eq!(requests.load(Ordering::SeqCst), 1);
    uv_snapshot!(context.filters(), context.tool_run()
        .args(args)
        .arg("--refresh")
        .arg("locked-tool"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The packaged lock for `locked-tool==1.0.0` contains unverified artifacts
      cause: URL for `idna==3.3` is not listed by http://[LOCALHOST]/simple: http://[LOCALHOST]/files/idna-3.3-py3-none-any.whl
    ");
    assert_eq!(requests.load(Ordering::SeqCst), 2);
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
    let hash_sha512 = hex::encode(Sha512::digest(&wheel));
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
    let requests_before = server
        .received_requests()
        .await
        .context("request recording disabled")?;
    let index_requests_before = requests_before
        .iter()
        .filter(|request| request.url.path() == "/simple/idna/")
        .count();
    context
        .tool_run()
        .args([
            "--locked",
            "--isolated",
            "--preview-features",
            "locked-tools",
            "--find-links",
            "wheels",
            "--index-url",
            &index,
            "locked-tool",
        ])
        .assert()
        .success();
    let requests_after = server
        .received_requests()
        .await
        .context("request recording disabled")?;
    assert_eq!(
        requests_after
            .iter()
            .filter(|request| request.url.path() == "/simple/idna/")
            .count(),
        index_requests_before
    );
    context
        .temp_dir
        .child("constraints.txt")
        .write_str("idna==3.4\n")?;
    uv_snapshot!(context.filters(), context.tool_run()
        .args(["--locked", "--preview-features", "locked-tools", "--find-links", "wheels",
            "--index-url", &index, "--constraint", "constraints.txt", "locked-tool"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The packaged lock selects `idna==3.3`, which is incompatible with constraint `idna==3.4`
    ");
    context
        .temp_dir
        .child("constraints.txt")
        .write_str("idna==3.4 ; extra == 'feature'\n")?;
    uv_snapshot!(context.filters(), context.tool_run()
        .args(["--locked", "--preview-features", "locked-tools", "--find-links", "wheels",
            "--index-url", &index, "--constraint", "constraints.txt", "locked-tool"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Constraints with extra markers are not supported with `--locked`
    ");
    context.temp_dir.child("constraints.txt").write_str(
        "idna==3.3 --hash=sha256:0000000000000000000000000000000000000000000000000000000000000000\n",
    )?;
    uv_snapshot!(context.filters(), context.tool_run()
        .args(["--locked", "--preview-features", "locked-tools", "--find-links", "wheels",
            "--index-url", &index, "--constraint", "constraints.txt", "locked-tool"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The selected artifact for `idna==3.3` does not match the required hashes
    ");
    context.temp_dir.child("constraints.txt").write_str(
        "locked-tool==1.0.0 --hash=sha256:0000000000000000000000000000000000000000000000000000000000000000\n",
    )?;
    context
        .tool_run()
        .args([
            "--locked",
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
        .assert()
        .failure();
    context
        .tool_run()
        .args([
            "--locked",
            "--preview-features",
            "locked-tools",
            "--index-url",
            &index,
            "--constraint",
            "constraints.txt",
            "--from",
            "./wheels/locked_tool-1.0.0-py3-none-any.whl",
            "locked-tool",
        ])
        .assert()
        .failure();
    let tool_hash = hex::encode(Sha256::digest(fs_err::read(
        context
            .temp_dir
            .child("wheels/locked_tool-1.0.0-py3-none-any.whl"),
    )?));
    context
        .temp_dir
        .child("constraints.txt")
        .write_str(&format!("locked-tool>=1 --hash=sha256:{tool_hash}\n"))?;
    context
        .tool_run()
        .args([
            "--locked",
            "--preview-features",
            "locked-tools",
            "--index-url",
            &index,
            "--constraint",
            "constraints.txt",
            "--from",
            "./wheels/locked_tool-1.0.0-py3-none-any.whl",
            "locked-tool",
        ])
        .assert()
        .success();
    context
        .tool_upgrade()
        .args([
            "--unlocked",
            "--preview-features",
            "locked-tools",
            "locked-tool",
        ])
        .env(EnvVars::UV_TOOL_LOCKED, "1")
        .env(EnvVars::PATH, bin_dir.as_os_str())
        .assert()
        .success();
    assert!(
        !context
            .read("tools/locked-tool/uv-receipt.toml")
            .contains("locked = true")
    );
    context
        .tool_upgrade()
        .args([
            "--locked",
            "--preview-features",
            "locked-tools",
            "locked-tool",
        ])
        .env(EnvVars::PATH, bin_dir.as_os_str())
        .assert()
        .success();
    assert!(
        context
            .read("tools/locked-tool/uv-receipt.toml")
            .contains("locked = true")
    );
    context
        .temp_dir
        .child("constraints.txt")
        .write_str(&format!("idna==3.3 --hash=sha256:{hash}\n"))?;
    context
        .tool_install()
        .args([
            "--unlocked",
            "--find-links",
            "wheels",
            "--index-url",
            &index,
            "--constraint",
            "constraints.txt",
            "locked-tool",
        ])
        .env(EnvVars::PATH, bin_dir.as_os_str())
        .assert()
        .success();
    assert!(
        !context
            .read("tools/locked-tool/uv-receipt.toml")
            .contains("locked = true")
    );
    context
        .tool_upgrade()
        .args([
            "--locked",
            "--preview-features",
            "locked-tools",
            "locked-tool",
        ])
        .env(EnvVars::PATH, bin_dir.as_os_str())
        .assert()
        .success();
    let invalid_hash = lock.replace(
        &hash,
        "0000000000000000000000000000000000000000000000000000000000000000",
    );
    tool(&context, "1.1.0", Some(&invalid_hash), None)?;
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .args(["--preview-features", "locked-tools", "locked-tool"])
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: Failed to upgrade locked-tool
      cause: Hash mismatch for `idna==3.3`

             Expected:
               sha256:0000000000000000000000000000000000000000000000000000000000000000

             Computed:
               sha256:b7a84ce11140568e6ef8f9ecd74ae5dee9dfab081a565ff422ebaa0d246bd078
    ");
    tool(&context, "1.2.0", Some(&lock), None)?;
    context
        .tool_upgrade()
        .args(["--preview-features", "locked-tools", "locked-tool"])
        .env(EnvVars::PATH, bin_dir.as_os_str())
        .assert()
        .success();
    assert!(
        context
            .read("tools/locked-tool/uv-receipt.toml")
            .contains("locked = true")
    );
    context
        .tool_upgrade()
        .args([
            "--unlocked",
            "--preview-features",
            "locked-tools",
            "locked-tool",
        ])
        .env(EnvVars::UV_TOOL_LOCKED, "1")
        .env(EnvVars::PATH, bin_dir.as_os_str())
        .assert()
        .success();
    assert!(
        !context
            .read("tools/locked-tool/uv-receipt.toml")
            .contains("locked = true")
    );
    let redirected = lock.replace(&url, &format!("{}/private/{filename}", server.uri()));
    tool(&context, "2.0.0", Some(&redirected), None)?;
    uv_snapshot!(context.filters(), context.tool_run()
        .args(["--locked", "--isolated", "--preview-features", "locked-tools", "--find-links",
            "wheels", "--index-url", &index, "locked-tool==2.0.0"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The packaged lock for `locked-tool==2.0.0` contains unverified artifacts
      cause: URL for `idna==3.3` is not listed by http://[LOCALHOST]/simple: http://[LOCALHOST]/private/idna-3.3-py3-none-any.whl
    ");
    let unconfigured = lock.replace(&index, "https://example.invalid/simple");
    tool(&context, "3.0.0", Some(&unconfigured), None)?;
    uv_snapshot!(context.filters(), context.tool_run()
        .args(["--locked", "--isolated", "--preview-features", "locked-tools", "--find-links",
            "wheels", "--index-url", &index, "locked-tool==3.0.0"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The packaged lock for `locked-tool==3.0.0` contains unverified artifacts
      cause: Index `https://example.invalid/simple` from the packaged lock is not configured
    ");
    assert!(
        !server
            .received_requests()
            .await
            .context("request recording disabled")?
            .iter()
            .any(|request| request.url.path().starts_with("/private/"))
    );
    let incorrect_sha512 = "0".repeat(128);
    let multiple_hashes = lock.replace(
        &format!("sha256 = \"{hash}\""),
        &format!("sha256 = \"{hash}\", sha512 = \"{incorrect_sha512}\""),
    );
    tool(&context, "4.0.0", Some(&multiple_hashes), None)?;
    context
        .temp_dir
        .child("constraints.txt")
        .write_str(&format!("idna==3.3 --hash=sha512:{incorrect_sha512}\n"))?;
    context
        .tool_run()
        .args([
            "--locked",
            "--isolated",
            "--preview-features",
            "locked-tools",
            "--find-links",
            "wheels",
            "--index-url",
            &index,
            "--constraint",
            "constraints.txt",
            "locked-tool==4.0.0",
        ])
        .assert()
        .failure();
    tool(&context, "5.0.0", Some(&lock), None)?;
    context
        .temp_dir
        .child("constraints.txt")
        .write_str(&format!("idna==3.3 --hash=sha512:{hash_sha512}\n"))?;
    context
        .tool_run()
        .args([
            "--locked",
            "--isolated",
            "--preview-features",
            "locked-tools",
            "--find-links",
            "wheels",
            "--index-url",
            &index,
            "--constraint",
            "constraints.txt",
            "locked-tool==5.0.0",
        ])
        .assert()
        .success();
    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn packaged_lock_install_run_upgrade() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix()
        .with_tool_dirs();
    context.temp_dir.child("wheels").create_dir_all()?;
    let bin_dir = context.temp_dir.child("bin");
    let lock = dependency(&context, "3.3")?;
    let newer_lock = dependency(&context, "3.4")?;
    tool(&context, "1.0.0", Some(&lock), None)?;
    // Compatibility may eliminate a release before its lock is considered.
    tool(&context, "2.0.0", None, Some(">=3.13"))?;

    uv_snapshot!(context.filters(), context.tool_install()
        .args(["locked-tool", "--find-links", "wheels"])
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved [N] packages in [TIME]
    Prepared [N] packages in [TIME]
    Installed [N] packages in [TIME]
     + idna==3.4
     + locked-tool==1.0.0
    Installed 1 executable: locked-tool
    ");

    uv_snapshot!(context.filters(), context.tool_run()
        .args(["--locked", "--preview-features", "locked-tools", "--find-links", "wheels", "locked-tool"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    3.3

    ----- stderr -----
    Installed [N] packages in [TIME]
     + idna==3.3
     + locked-tool==1.0.0
    ");

    // An existing installation resolved normally must be brought back to the packaged pins.
    uv_snapshot!(context.filters(), context.tool_install()
        .args(["--locked", "--preview-features", "locked-tools", "locked-tool", "--find-links", "wheels"])
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Uninstalled [N] packages in [TIME]
    Installed [N] packages in [TIME]
     - idna==3.4
     + idna==3.3
     ~ locked-tool==1.0.0
    Installed 1 executable: locked-tool
    ");
    insta::with_settings!({ filters => context.filters() }, {
        assert_snapshot!(context.read("tools/locked-tool/uv-receipt.toml"), @r#"
        [tool]
        locked = true
        requirements = [{ name = "locked-tool" }]
        entrypoints = [
            { name = "locked-tool", install-path = "[TEMP_DIR]/bin/locked-tool", from = "locked-tool" },
        ]

        [tool.options]
        find-links = ["file://[TEMP_DIR]/wheels"]
        exclude-newer = "2024-03-25T00:00:00Z"
        "#);
    });

    uv_snapshot!(context.filters(), context.tool_install()
        .args(["locked-tool", "--find-links", "wheels"])
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    exit_code: 0 (success)
    ----- stderr -----
    `locked-tool` is already installed
    ");
    assert!(
        context
            .read("tools/locked-tool/uv-receipt.toml")
            .contains("locked = true")
    );

    tool(&context, "1.1.0", Some(&newer_lock), None)?;
    // The receipt keeps upgrades locked even without repeating --locked.
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .args(["--preview-features", "locked-tools", "locked-tool"])
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Updated locked-tool v1.0.0 -> v1.1.0
     - idna==3.3
     + idna==3.4
     - locked-tool==1.0.0
     + locked-tool==1.1.0
    Installed 1 executable: locked-tool
    ");
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .args(["--locked", "--preview-features", "locked-tools", "locked-tool"])
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Modified locked-tool environment
     ~ idna==3.4
     ~ locked-tool==1.1.0
    Installed 1 executable: locked-tool
    ");

    // An ordinary install can change saved options without changing the environment.
    context
        .tool_install()
        .args([
            "--no-binary-package",
            "locked-tool",
            "--find-links",
            "wheels",
            "locked-tool",
        ])
        .env(EnvVars::PATH, bin_dir.as_os_str())
        .assert()
        .success();
    assert!(
        !context
            .read("tools/locked-tool/uv-receipt.toml")
            .contains("locked = true")
    );
    context
        .tool_install()
        .args([
            "--locked",
            "--preview-features",
            "locked-tools",
            "--find-links",
            "wheels",
            "locked-tool",
        ])
        .env(EnvVars::PATH, bin_dir.as_os_str())
        .assert()
        .success();

    // Adding a requirement changes the receipt even if it is already installed.
    context
        .tool_install()
        .args([
            "--preview-features",
            "tool-install-locks",
            "--with",
            "idna==3.4",
            "--find-links",
            "wheels",
            "locked-tool",
        ])
        .env(EnvVars::PATH, bin_dir.as_os_str())
        .assert()
        .success();
    assert!(
        !context
            .read("tools/locked-tool/uv-receipt.toml")
            .contains("locked = true")
    );
    Ok(())
}

#[test]
fn packaged_lock_required() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix()
        .with_tool_dirs();
    context.temp_dir.child("wheels").create_dir_all()?;
    let lock = indoc! {r#"
        lock-version = "1.0"
        created-by = "test"
        requires-python = ">=3.12"
        packages = []
    "#};
    tool(&context, "1.0.0", Some(lock), None)?;
    tool(&context, "2.0.0", None, None)?;

    uv_snapshot!(context.filters(), context.tool_install().args(["--locked", "locked-tool"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: `--locked` for tools requires the `locked-tools` preview feature
    ");
    uv_snapshot!(context.filters(), context.tool_run().args(["--locked", "locked-tool"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: `--locked` for tools requires the `locked-tools` preview feature
    ");
    uv_snapshot!(context.filters(), context.tool_upgrade().args(["--locked", "locked-tool"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: `--locked` for tools requires the `locked-tools` preview feature
    ");
    uv_snapshot!(context.filters(), context.tool_run()
        .args(["--locked", "--preview-features", "locked-tools", "python"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: `--locked` requires a tool package with a bundled lock, not a Python interpreter
    ");
    // A compatible release without a lock cannot cause a fallback to an older release.
    uv_snapshot!(context.filters(), context.tool_install()
        .args(["--locked", "--preview-features", "locked-tools", "--no-index", "--find-links", "wheels", "locked-tool"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: `locked-tool==2.0.0` does not contain `pylock.toml` in its `.dist-info` directory; `--locked` requires a packaged lock
    ");

    tool(&context, "3.0.0", Some("not valid TOML"), None)?;
    uv_snapshot!(context.filters(), context.tool_run()
        .args(["--locked", "--preview-features", "locked-tools", "--no-index", "--find-links", "wheels", "locked-tool==3.0.0"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: `locked-tool==3.0.0` contains an invalid `pylock.toml`
      cause: TOML parse error at line 1, column 5
               |
             1 | not valid TOML
               |     ^
             key with no value, expected `=`
    ");

    tool(
        &context,
        "4.0.0",
        Some(&lock.replace(">=3.12", ">=3.13")),
        None,
    )?;
    uv_snapshot!(context.filters(), context.tool_install()
        .args(["--locked", "--preview-features", "locked-tools", "--no-index", "--find-links", "wheels", "locked-tool==4.0.0"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The requested interpreter resolved to Python 3.12.[X], which is incompatible with the `pylock.toml`'s Python requirement: `>=3.13`
    ");

    uv_snapshot!(context.filters(), context.tool_install()
        .args(["--locked", "--preview-features", "locked-tools", "--no-index", "--find-links", "wheels", "--with", "locked-dependency", "locked-tool==1.0.0"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: `--locked` requires a single tool package and cannot be combined with `--with`
    ");
    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn packaged_lock_hashes() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix()
        .with_tool_dirs();
    context.temp_dir.child("wheels").create_dir_all()?;
    let lock = dependency(&context, "3.3")?;
    tool(&context, "1.0.0", Some(&lock), None)?;
    // Change the expected digest while retaining a valid lock and artifact.
    let mut lock: toml::Value = toml::from_str(&lock)?;
    lock["packages"][0]["wheels"][0]["hashes"]["sha256"] = toml::Value::String("0".repeat(64));
    tool(&context, "2.0.0", Some(&toml::to_string(&lock)?), None)?;
    let bin_dir = context.temp_dir.child("bin");
    context
        .tool_install()
        .args([
            "--locked",
            "--preview-features",
            "locked-tools",
            "--find-links",
            "wheels",
            "locked-tool==1.0.0",
        ])
        .env(EnvVars::PATH, bin_dir.as_os_str())
        .assert()
        .success();
    uv_snapshot!(context.filters(), context.tool_run()
        .args(["--locked", "--preview-features", "locked-tools", "--find-links", "wheels", "locked-tool==2.0.0"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Hash mismatch for `idna==3.3`

    Expected:
      sha256:0000000000000000000000000000000000000000000000000000000000000000

    Computed:
      sha256:84d9dd047ffa80596e0f246e2eab0b391788b0503584e8945f2368256d2735ff
    ");

    lock["packages"][0]["wheels"][0]["hashes"] = toml::Value::Table(toml::Table::new());
    tool(&context, "3.0.0", Some(&toml::to_string(&lock)?), None)?;
    uv_snapshot!(context.filters(), context.tool_run()
        .args(["--locked", "--preview-features", "locked-tools", "--find-links", "wheels", "locked-tool==3.0.0"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    warning: Empty hash tables in `pylock.toml` will be rejected in a future uv version. Rerun the original `uv export` or `uv pip compile` command to regenerate the file.
    error: The packaged lock for `locked-tool==3.0.0` is missing artifact hashes; regenerate the lock before publishing the package
    ");
    Ok(())
}

#[tokio::test]
#[cfg(feature = "test-pypi")]
async fn packaged_lock_index_cache() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix()
        .with_tool_dirs();
    context.temp_dir.child("wheels").create_dir_all()?;
    let server = MockServer::start().await;
    let index = format!("{}/simple", server.uri());
    let (filename, private_wheel) = generate_wheel_with_files(
        &"idna".parse()?,
        &"3.3".parse()?,
        &[],
        &BTreeMap::new(),
        None,
        "py3-none-any",
        &[("idna/private.py", "")],
    );
    let simple = json!({
        "meta": { "api-version": "1.1" },
        "name": "idna",
        "files": [{
            "filename": filename,
            "url": format!("{}/files/{filename}", server.uri()),
            "hashes": {},
            "core-metadata": true,
            "upload-time": "2024-03-24T00:00:00Z"
        }]
    });
    Mock::given(method("GET"))
        .and(path("/simple/idna/"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(simple.to_string(), "application/vnd.pypi.simple.v1+json"),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/files/{filename}.metadata")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("Metadata-Version: 2.1\nName: idna\nVersion: 3.3\n\n"),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/files/{filename}")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(private_wheel))
        .mount(&server)
        .await;
    let mut lock: toml::Value = toml::from_str(&dependency(&context, "3.3")?)?;
    lock["packages"][0]
        .as_table_mut()
        .context("Expected a package table")?
        .insert("index".to_owned(), toml::Value::String(index.clone()));
    tool(&context, "1.0.0", Some(&toml::to_string(&lock)?), None)?;

    uv_snapshot!(context.filters(), context.tool_run()
        .args(["--locked", "--preview-features", "locked-tools", "--find-links", "wheels", "locked-tool"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The packaged lock for `locked-tool==1.0.0` contains unverified artifacts
      cause: Index `http://[LOCALHOST]/simple` from the packaged lock is not configured
    ");

    context
        .pip_install()
        .args(["--index-url", &index, "idna==3.3"])
        .assert()
        .success();
    uv_snapshot!(context.python_command()
        .args(["-c", "import importlib.util; print('private' if importlib.util.find_spec('idna.private') else 'pypi')"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    private
    ");
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
    uv_snapshot!(context.filters(), context.tool_run()
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
fn packaged_lock_from_build() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix()
        .with_tool_dirs();
    context.temp_dir.child("src/locked_tool").create_dir_all()?;
    context
        .temp_dir
        .child("src/locked_tool/__init__.py")
        .write_str("import idna\ndef main(): print(idna.__version__)\n")?;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "locked-tool"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = ["idna>=3.4"]
        [project.scripts]
        locked-tool = "locked_tool:main"
        [build-system]
        requires = ["uv_build>=0.5.15,<2"]
        build-backend = "uv_build"
        [tool.uv]
        resolution = "lowest-direct"
    "#})?;
    context.lock().assert().success();
    context
        .build()
        .env(EnvVars::UV_EXPORT_LOCK, "true")
        .args(["--preview-features", "locked-tools", "--wheel"])
        .assert()
        .success();
    let bin_dir = context.temp_dir.child("bin");
    uv_snapshot!(context.filters(), context.tool_install()
        .args(["--locked", "--preview-features", "locked-tools", "--from", "./dist/locked_tool-1.0.0-py3-none-any.whl", "locked-tool"])
        .env(EnvVars::PATH, bin_dir.as_os_str()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Installed [N] packages in [TIME]
     + idna==3.4
     + locked-tool==1.0.0 (from file://[TEMP_DIR]/dist/locked_tool-1.0.0-py3-none-any.whl)
    Installed 1 executable: locked-tool
    ");
    uv_snapshot!(context.filters(), context.tool_run()
        .args(["--locked", "--preview-features", "locked-tools", "--offline", "--from", "./dist/locked_tool-1.0.0-py3-none-any.whl", "locked-tool"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    3.4

    ----- stderr -----
    Installed [N] packages in [TIME]
     + idna==3.4
     + locked-tool==1.0.0 (from file://[TEMP_DIR]/dist/locked_tool-1.0.0-py3-none-any.whl)
    ");
    Ok(())
}

#[tokio::test]
async fn packaged_lock_from_build_with_extras() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_filtered_exe_suffix()
        .with_tool_dirs();
    let server = MockServer::start().await;
    let index = format!("{}/simple", server.uri());
    for (name, dependencies, extras) in [
        (
            "dependency",
            vec![],
            BTreeMap::from([("feature".parse()?, vec!["leaf".parse()?])]),
        ),
        (
            "helper",
            vec!["dependency[feature]".parse()?],
            BTreeMap::new(),
        ),
        ("leaf", vec![], BTreeMap::new()),
    ] {
        let (filename, wheel) = generate_wheel_with_files(
            &name.parse()?,
            &"1.0.0".parse()?,
            &dependencies,
            &extras,
            None,
            "py3-none-any",
            &[],
        );
        let hash = hex::encode(Sha256::digest(&wheel));
        let url = format!("{}/files/{filename}", server.uri());
        Mock::given(method("GET"))
            .and(path(format!("/files/{filename}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(wheel))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/simple/{name}/")))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(
                    json!({"meta": {"api-version": "1.1"}, "name": name, "files": [{
                        "filename": filename, "url": url, "hashes": {"sha256": hash},
                        "upload-time": "2023-01-01T00:00:00Z"
                    }]})
                    .to_string(),
                    "application/vnd.pypi.simple.v1+json",
                ),
            )
            .mount(&server)
            .await;
    }
    context.temp_dir.child("src/locked_tool").create_dir_all()?;
    context
        .temp_dir
        .child("src/locked_tool/__init__.py")
        .write_str(indoc! {r#"
            from importlib.metadata import PackageNotFoundError, version
            def main():
                for name in ("dependency", "helper", "leaf"):
                    try:
                        print(f"{name}=={version(name)}")
                    except PackageNotFoundError:
                        pass
        "#})?;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "locked-tool"
        version = "1.0.0"
        requires-python = ">=3.12"
        dependencies = ["dependency"]
        [project.optional-dependencies]
        fast = ["helper"]
        [project.scripts]
        locked-tool = "locked_tool:main"
        [build-system]
        requires = ["uv_build>=0.5.15,<2"]
        build-backend = "uv_build"
    "#})?;
    context
        .lock()
        .args(["--index-url", &index])
        .assert()
        .success();
    context
        .build()
        .env(EnvVars::UV_EXPORT_LOCK, "true")
        .args([
            "--preview-features",
            "locked-tools",
            "--index-url",
            &index,
            "--wheel",
        ])
        .assert()
        .success();
    uv_snapshot!(context.filters(), context.tool_run().args([
        "--locked", "--isolated", "--preview-features", "locked-tools", "--index-url", &index,
        "--find-links", "dist", "locked-tool"
    ]), @"
    exit_code: 0 (success)
    ----- stdout -----
    dependency==1.0.0

    ----- stderr -----
    Installed [N] packages in [TIME]
     + dependency==1.0.0
     + locked-tool==1.0.0
    ");
    uv_snapshot!(context.filters(), context.tool_run().args([
        "--locked", "--isolated", "--preview-features", "locked-tools", "--index-url", &index,
        "--find-links", "dist", "locked-tool[fast]"
    ]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Extras are not supported with `--locked`
    ");
    Ok(())
}

async fn mount_locked_artifact(
    server: &MockServer,
    route: &str,
    filename: &str,
    bytes: &[u8],
    index_hash: &str,
    requires_python: Option<&str>,
) -> (String, String) {
    let index = format!("{}/{route}/simple", server.uri());
    // An index may use URLs that do not contain the artifact's filename.
    let url = format!("{}/{route}/artifact", server.uri());
    let mut file = json!({
        "filename": filename,
        "url": url,
        "hashes": { "sha256": index_hash },
    });
    if let Some(requires_python) = requires_python {
        file["requires-python"] = json!(requires_python);
    }
    Mock::given(method("GET"))
        .and(path(format!("/{route}/simple/idna/")))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(
                json!({
                    "meta": { "api-version": "1.1" },
                    "name": "idna",
                    "files": [file],
                })
                .to_string(),
                "application/vnd.pypi.simple.v1+json",
            ),
        )
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/{route}/artifact")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes.to_vec()))
        .mount(server)
        .await;
    (index, url)
}

fn locked_artifact(index: &str, url: &str, filename: &str, hashes: &str, kind: &str) -> String {
    let artifact = format!(r#"{{ name = "{filename}", url = "{url}", hashes = {{ {hashes} }} }}"#);
    let artifact = if kind == "wheels" {
        format!("[{artifact}]")
    } else {
        artifact
    };
    formatdoc! {r#"
        lock-version = "1.0"
        created-by = "test"
        [[packages]]
        name = "idna"
        version = "3.3"
        index = "{index}"
        {kind} = {artifact}
    "#}
}

#[tokio::test]
async fn packaged_lock_rejects_weak_hashes() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_tool_dirs();
    context.temp_dir.child("wheels").create_dir_all()?;
    let server = MockServer::start().await;
    let (filename, wheel) = generate_wheel_with_files(
        &"idna".parse()?,
        &"3.3".parse()?,
        &[],
        &BTreeMap::new(),
        None,
        "py3-none-any",
        &[],
    );
    let sha256 = hex::encode(Sha256::digest(&wheel));
    let mut hasher = Hasher::from(HashAlgorithm::Md5);
    hasher.update(&wheel);
    let md5 = HashDigest::from(hasher).to_string();
    let md5 = md5.strip_prefix("md5:").context("Expected an MD5 hash")?;
    let (index, url) =
        mount_locked_artifact(&server, "hashes", &filename, &wheel, &sha256, None).await;
    let context = context.with_filter((sha256.clone(), "[SHA256]"));
    let weak = locked_artifact(
        &index,
        &url,
        &filename,
        &format!("md5 = \"{md5}\""),
        "wheels",
    );
    let mixed = locked_artifact(
        &index,
        &url,
        &filename,
        &format!("md5 = \"{md5}\", sha256 = \"{}\"", "0".repeat(64)),
        "wheels",
    );
    let strong = locked_artifact(
        &index,
        &url,
        &filename,
        &format!("md5 = \"{md5}\", sha256 = \"{sha256}\""),
        "wheels",
    );
    tool(&context, "1.0.0", Some(&weak), None)?;
    tool(&context, "2.0.0", Some(&mixed), None)?;
    tool(&context, "3.0.0", Some(&strong), None)?;
    uv_snapshot!(context.filters(), context.tool_run().args(["--locked", "--isolated", "--preview-features", "locked-tools", "--find-links", "wheels", "--index-url", &index, "locked-tool==1.0.0"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The packaged lock for `locked-tool==1.0.0` has no secure hash for `idna==3.3`
    ");
    uv_snapshot!(context.filters(), context.tool_run().args(["--locked", "--isolated", "--preview-features", "locked-tools", "--find-links", "wheels", "--index-url", &index, "locked-tool==2.0.0"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Hash mismatch for `idna==3.3`

    Expected:
      sha256:0000000000000000000000000000000000000000000000000000000000000000

    Computed:
      sha256:[SHA256]
    ");
    context
        .tool_run()
        .args([
            "--locked",
            "--isolated",
            "--preview-features",
            "locked-tools",
            "--find-links",
            "wheels",
            "--index-url",
            &index,
            "locked-tool==3.0.0",
        ])
        .assert()
        .success();
    Ok(())
}

#[tokio::test]
async fn packaged_lock_checks_index_filename() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_tool_dirs();
    context.temp_dir.child("wheels").create_dir_all()?;
    let server = MockServer::start().await;
    let (filename, wheel) = generate_wheel_with_files(
        &"idna".parse()?,
        &"3.3".parse()?,
        &[],
        &BTreeMap::new(),
        None,
        "cp313-cp313-win_amd64",
        &[],
    );
    let hash = hex::encode(Sha256::digest(&wheel));
    let (index, url) =
        mount_locked_artifact(&server, "filename", &filename, &wheel, &hash, None).await;
    let lock = locked_artifact(
        &index,
        &url,
        "idna-3.3-py3-none-any.whl",
        &format!("sha256 = \"{hash}\""),
        "wheels",
    );
    tool(&context, "1.0.0", Some(&lock), None)?;
    uv_snapshot!(context.filters(), context.tool_run().args(["--locked", "--isolated", "--preview-features", "locked-tools", "--find-links", "wheels", "--index-url", &index, "locked-tool"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The packaged lock for `locked-tool==1.0.0` contains unverified artifacts
      cause: Filename `idna-3.3-py3-none-any.whl` for `idna==3.3` does not match the file listed at http://[LOCALHOST]/filename/artifact by http://[LOCALHOST]/filename/simple
    ");
    assert!(
        !server
            .received_requests()
            .await
            .context("request recording disabled")?
            .iter()
            .any(|request| request.url.path() == "/filename/artifact")
    );
    let sdist = generate_source_archive(&"idna".parse()?, &"3.3".parse()?, "", None)?;
    let hash = hex::encode(Sha256::digest(&sdist));
    let (index, url) = mount_locked_artifact(
        &server,
        "source-name",
        "idna-3.3.tar.gz",
        &sdist,
        &hash,
        None,
    )
    .await;
    let lock = locked_artifact(
        &index,
        &url,
        "idna-3.3.zip",
        &format!("sha256 = \"{hash}\""),
        "sdist",
    );
    tool(&context, "2.0.0", Some(&lock), None)?;
    uv_snapshot!(context.filters(), context.tool_run().args(["--locked", "--isolated", "--preview-features", "locked-tools", "--find-links", "wheels", "--index-url", &index, "locked-tool==2.0.0"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The packaged lock for `locked-tool==2.0.0` contains unverified artifacts
      cause: Filename `idna-3.3.zip` for `idna==3.3` does not match the file listed at http://[LOCALHOST]/source-name/artifact by http://[LOCALHOST]/source-name/simple
    ");
    assert!(
        !server
            .received_requests()
            .await
            .context("request recording disabled")?
            .iter()
            .any(|request| request.url.path() == "/source-name/artifact")
    );
    Ok(())
}

#[tokio::test]
async fn packaged_lock_checks_dependency_python() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_tool_dirs();
    context.temp_dir.child("wheels").create_dir_all()?;
    let server = MockServer::start().await;
    let (filename, wheel) = generate_wheel_with_files(
        &"idna".parse()?,
        &"3.3".parse()?,
        &[],
        &BTreeMap::new(),
        Some(&">=99".parse()?),
        "py3-none-any",
        &[],
    );
    let hash = hex::encode(Sha256::digest(&wheel));
    let (indexed, indexed_url) =
        mount_locked_artifact(&server, "indexed", &filename, &wheel, &hash, Some(">=99")).await;
    let (unindexed, unindexed_url) =
        mount_locked_artifact(&server, "unindexed", &filename, &wheel, &hash, None).await;
    let hashes = format!("sha256 = \"{hash}\"");
    tool(
        &context,
        "1.0.0",
        Some(&locked_artifact(
            &indexed,
            &indexed_url,
            &filename,
            &hashes,
            "wheels",
        )),
        None,
    )?;
    tool(
        &context,
        "2.0.0",
        Some(&locked_artifact(
            &unindexed,
            &unindexed_url,
            &filename,
            &hashes,
            "wheels",
        )),
        None,
    )?;
    uv_snapshot!(context.filters(), context.tool_run().args(["--locked", "--isolated", "--preview-features", "locked-tools", "--find-links", "wheels", "--index-url", &indexed, "locked-tool==1.0.0"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: `idna==3.3` requires Python `>=99`, but the selected interpreter is Python 3.12.[X]
    ");
    uv_snapshot!(context.filters(), context.tool_run().args(["--locked", "--isolated", "--preview-features", "locked-tools", "--find-links", "wheels", "--index-url", &unindexed, "locked-tool==2.0.0"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: `idna==3.3` requires Python `>=99`, but the selected interpreter is Python 3.12.[X]
    ");
    Ok(())
}

#[tokio::test]
async fn packaged_lock_checks_index_hash() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_tool_dirs();
    context.temp_dir.child("wheels").create_dir_all()?;
    let server = MockServer::start().await;
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
    let (index, url) = mount_locked_artifact(
        &server,
        "index-hash",
        &filename,
        &wheel,
        &"0".repeat(64),
        None,
    )
    .await;
    let lock = locked_artifact(
        &index,
        &url,
        &filename,
        &format!("sha256 = \"{hash}\""),
        "wheels",
    );
    tool(&context, "1.0.0", Some(&lock), None)?;
    uv_snapshot!(context.filters(), context.tool_run().args(["--locked", "--isolated", "--preview-features", "locked-tools", "--find-links", "wheels", "--index-url", &index, "locked-tool"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The selected artifact for `idna==3.3` does not match the required hashes
    ");
    Ok(())
}

#[tokio::test]
async fn packaged_lock_checks_override_wheel_before_reading_dependencies() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_tool_dirs();
    context.temp_dir.child("wheels").create_dir_all()?;
    tool(
        &context,
        "1.0.0",
        Some("lock-version = \"1.0\"\ncreated-by = \"test\"\npackages = []\n"),
        None,
    )?;
    let (filename, wheel) = generate_wheel_with_files(
        &"idna".parse()?,
        &"3.5".parse()?,
        &["unexpected==1.0".parse()?],
        &BTreeMap::new(),
        None,
        "py3-none-any",
        &[],
    );
    let hash = hex::encode(Sha256::digest(&wheel));
    let context = context.with_filter((hash.clone(), "[WHEEL_HASH]"));
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/simple/idna/"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(
                json!({"meta": {"api-version": "1.1"}, "name": "idna", "files": [{
                    "filename": filename,
                    "url": format!("{}/{filename}", server.uri()),
                    "hashes": {"sha256": hash},
                    "upload-time": "2023-01-01T00:00:00Z",
                }]})
                .to_string(),
                "application/vnd.pypi.simple.v1+json",
            ),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/{filename}")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(wheel))
        .mount(&server)
        .await;
    context.temp_dir.child("overrides.txt").write_str(
        "idna==3.5 --hash=sha256:0000000000000000000000000000000000000000000000000000000000000000\n",
    )?;
    uv_snapshot!(context.filters(), context.tool_run().args(["--locked", "--isolated", "--preview-features", "locked-tools", "--find-links", "wheels", "--index-url", &format!("{}/simple", server.uri()), "--override", "overrides.txt", "locked-tool"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to download `idna==3.5`
      cause: Hash mismatch for `idna==3.5`

             Expected:
               sha256:0000000000000000000000000000000000000000000000000000000000000000
               sha256:[WHEEL_HASH]

             Computed:
               sha256:[WHEEL_HASH]
    ");
    let requests = server
        .received_requests()
        .await
        .context("request recording disabled")?;
    assert!(
        !requests
            .iter()
            .any(|request| request.url.path() == "/simple/unexpected/")
    );
    Ok(())
}

#[tokio::test]
async fn packaged_lock_checks_override_hash_before_building() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_tool_dirs();
    context.temp_dir.child("wheels").create_dir_all()?;
    let marker = context.temp_dir.child("backend-executed");
    let sdist =
        generate_source_archive(&"idna".parse()?, &"3.5".parse()?, "", Some(marker.path()))?;
    let hash = hex::encode(Sha256::digest(&sdist));
    let hash512 = hex::encode(Sha512::digest(&sdist));
    let context = context
        .with_filter((hash.clone(), "[SDIST_HASH]"))
        .with_filter((hash512.clone(), "[SDIST_SHA512]"));
    let server = MockServer::start().await;
    let index = format!("{}/simple", server.uri());
    Mock::given(method("GET"))
        .and(path("/simple/idna/"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(
                json!({"meta": {"api-version": "1.1"}, "name": "idna", "files": [{
                    "filename": "idna-3.5.tar.gz",
                    "url": format!("{}/idna-3.5.tar.gz", server.uri()),
                    "hashes": {"sha256": hash},
                    "upload-time": "2023-01-01T00:00:00Z",
                }]})
                .to_string(),
                "application/vnd.pypi.simple.v1+json",
            ),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/idna-3.5.tar.gz"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(sdist.clone()))
        .mount(&server)
        .await;
    tool(
        &context,
        "1.0.0",
        Some("lock-version = \"1.0\"\ncreated-by = \"test\"\npackages = []\n"),
        None,
    )?;
    context.temp_dir.child("overrides.txt").write_str(
        "idna==3.5 --hash=sha256:0000000000000000000000000000000000000000000000000000000000000000\n",
    )?;
    uv_snapshot!(context.filters(), context.tool_run().args(["--locked", "--isolated", "--preview-features", "locked-tools", "--find-links", "wheels", "--index-url", &index, "--override", "overrides.txt", "locked-tool"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to download and build `idna==3.5`
      cause: Hash mismatch for `idna==3.5`

             Expected:
               sha256:0000000000000000000000000000000000000000000000000000000000000000
               sha256:[SDIST_HASH]

             Computed:
               sha256:[SDIST_HASH]
    ");
    assert!(!marker.path().exists());
    let (target, platform) = if cfg!(windows) {
        ("linux", "linux")
    } else {
        ("windows", "win32")
    };
    context.temp_dir.child("overrides.txt").write_str(&format!(
        "idna==3.5 ; sys_platform == '{platform}' --hash=sha256:{}\n",
        "0".repeat(64)
    ))?;
    uv_snapshot!(context.filters(), context.tool_run().args(["--locked", "--isolated", "--preview-features", "locked-tools", "--find-links", "wheels", "--index-url", &index, "--python-platform", target, "--override", "overrides.txt", "locked-tool"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to download and build `idna==3.5`
      cause: Hash mismatch for `idna==3.5`

             Expected:
               sha256:0000000000000000000000000000000000000000000000000000000000000000
               sha256:[SDIST_HASH]

             Computed:
               sha256:[SDIST_HASH]
    ");
    assert!(!marker.path().exists());
    let archive = context.temp_dir.child("idna-3.5.tar.gz");
    archive.write_binary(&sdist)?;
    let archive_url = url::Url::from_file_path(archive.path())
        .map_err(|()| anyhow::anyhow!("invalid archive path"))?;
    context
        .temp_dir
        .child("overrides.txt")
        .write_str(&format!("idna @ {archive_url}\n"))?;
    context
        .temp_dir
        .child("constraints.txt")
        .write_str(&format!("idna==3.5 --hash=sha256:{}\n", "0".repeat(64)))?;
    let output = context
        .tool_run()
        .args([
            "--locked",
            "--isolated",
            "--preview-features",
            "locked-tools",
            "--find-links",
            "wheels",
            "--index-url",
            &index,
            "--override",
            "overrides.txt",
            "--constraint",
            "constraints.txt",
            "locked-tool",
        ])
        .output()?;
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Hash mismatch"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!marker.path().exists());

    context.temp_dir.child("overrides.txt").write_str(&format!(
        "idna @ {archive_url}#sha256={hash}&sha512={}\n",
        "0".repeat(128)
    ))?;
    let output = context
        .tool_run()
        .args([
            "--locked",
            "--isolated",
            "--preview-features",
            "locked-tools",
            "--find-links",
            "wheels",
            "--index-url",
            &index,
            "--override",
            "overrides.txt",
            "locked-tool",
        ])
        .output()?;
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Hash mismatch"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!marker.path().exists());

    let unnamed_url = format!("{}/unexpected.tar.gz", server.uri());
    context
        .temp_dir
        .child("overrides.txt")
        .write_str(&format!("{unnamed_url}#sha256={}\n", "0".repeat(64)))?;
    let output = context
        .tool_run()
        .args([
            "--locked",
            "--isolated",
            "--preview-features",
            "locked-tools",
            "--find-links",
            "wheels",
            "--index-url",
            &index,
            "--override",
            "overrides.txt",
            "locked-tool",
        ])
        .output()?;
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("An override with hashes must include its package name"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!marker.path().exists());

    let bad_index = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/simple/idna/"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(
                json!({"meta": {"api-version": "1.1"}, "name": "idna", "files": [{
                    "filename": "idna-3.5.tar.gz",
                    "url": format!("{}/idna-3.5.tar.gz", bad_index.uri()),
                    "hashes": {"sha256": hash, "sha512": "0".repeat(128)},
                    "upload-time": "2023-01-01T00:00:00Z",
                }]})
                .to_string(),
                "application/vnd.pypi.simple.v1+json",
            ),
        )
        .mount(&bad_index)
        .await;
    Mock::given(method("GET"))
        .and(path("/idna-3.5.tar.gz"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(sdist.clone()))
        .mount(&bad_index)
        .await;
    context
        .temp_dir
        .child("overrides.txt")
        .write_str(&format!("idna==3.5 --hash=sha256:{hash}\n"))?;
    let bad_index_url = format!("{}/simple", bad_index.uri());
    uv_snapshot!(context.filters(), context.tool_run().args(["--locked", "--isolated", "--preview-features", "locked-tools", "--find-links", "wheels", "--index-url", &bad_index_url, "--override", "overrides.txt", "locked-tool"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to download and build `idna==3.5`
      cause: Hash mismatch for `idna==3.5`

             Expected:
               sha256:[SDIST_HASH]
               sha512:00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
               sha256:[SDIST_HASH]

             Computed:
               sha256:[SDIST_HASH]
               sha512:[SDIST_SHA512]
    ");
    assert!(!marker.path().exists());

    Mock::given(method("GET"))
        .and(path("/url-hash/idna/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            json!({"meta": {"api-version": "1.1"}, "name": "idna", "files": [{
                "filename": "idna-3.5.tar.gz",
                "url": format!("{}/idna-3.5.tar.gz#sha512={}", bad_index.uri(), "0".repeat(128)),
                "hashes": {"sha256": hash},
                "upload-time": "2023-01-01T00:00:00Z",
            }]})
            .to_string(),
            "application/vnd.pypi.simple.v1+json",
        ))
        .mount(&bad_index)
        .await;
    let fragment_index_url = format!("{}/url-hash", bad_index.uri());
    uv_snapshot!(context.filters(), context.tool_run().args(["--locked", "--isolated", "--preview-features", "locked-tools", "--find-links", "wheels", "--index-url", &fragment_index_url, "--override", "overrides.txt", "locked-tool"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to download and build `idna==3.5`
      cause: Hash mismatch for `idna==3.5`

             Expected:
               sha256:[SDIST_HASH]
               sha256:[SDIST_HASH]
               sha512:00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000

             Computed:
               sha256:[SDIST_HASH]
               sha512:[SDIST_SHA512]
    ");
    assert!(!marker.path().exists());

    context
        .temp_dir
        .child("overrides.txt")
        .write_str("idna==3.5\n")?;
    context
        .temp_dir
        .child("constraints.txt")
        .write_str(&format!(
            "idna==3.5 --hash=sha256:{}\nidna==3.5 --hash=sha256:{hash}\n",
            "0".repeat(64)
        ))?;
    uv_snapshot!(context.filters(), context.tool_run().args(["--locked", "--isolated", "--preview-features", "locked-tools", "--find-links", "wheels", "--index-url", &index, "--override", "overrides.txt", "--constraint", "constraints.txt", "locked-tool"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to download and build `idna==3.5`
      cause: Hash mismatch for `idna==3.5`

             Expected:
               sha256:0000000000000000000000000000000000000000000000000000000000000000
               sha256:[SDIST_HASH]
               sha256:[SDIST_HASH]

             Computed:
               sha256:[SDIST_HASH]
    ");
    assert!(!marker.path().exists());
    Ok(())
}

#[tokio::test]
async fn packaged_lock_checks_sdist_constraints_before_building() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_tool_dirs();
    context.temp_dir.child("wheels").create_dir_all()?;
    let marker = context.temp_dir.child("backend-executed");
    let sdist =
        generate_source_archive(&"idna".parse()?, &"3.3".parse()?, "", Some(marker.path()))?;
    let hash = hex::encode(Sha256::digest(&sdist));
    let lock_hash = hex::encode(Sha512::digest(&sdist));
    let constraint_hash = hex::encode(Sha384::digest(&sdist));
    let context = context
        .with_filter((hash.clone(), "[INDEX_HASH]"))
        .with_filter((lock_hash.clone(), "[LOCK_HASH]"))
        .with_filter((constraint_hash.clone(), "[CONSTRAINT_HASH]"));
    let server = MockServer::start().await;
    let filename = "idna-3.3.tar.gz";
    let (index, url) = mount_locked_artifact(&server, "sdist", filename, &sdist, &hash, None).await;
    let lock = locked_artifact(
        &index,
        &url,
        filename,
        &format!("sha512 = \"{lock_hash}\""),
        "sdist",
    );
    tool(&context, "1.0.0", Some(&lock), None)?;
    context
        .temp_dir
        .child("constraints.txt")
        .write_str(&format!("idna==3.3 --hash=sha384:{}\n", "0".repeat(96)))?;
    uv_snapshot!(context.filters(), context.tool_run().args(["--locked", "--isolated", "--preview-features", "locked-tools", "--find-links", "wheels", "--index-url", &index, "--constraint", "constraints.txt", "locked-tool"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Hash mismatch for `idna==3.3`

    Expected:
      sha512:[LOCK_HASH]
      sha384:000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
      sha256:[INDEX_HASH]

    Computed:
      sha256:[INDEX_HASH]
      sha384:[CONSTRAINT_HASH]
      sha512:[LOCK_HASH]
    ");
    assert!(!marker.path().exists());
    let mut alternatives = String::new();
    for value in 0..130 {
        write!(&mut alternatives, " --hash=sha384:{value:096x}")?;
    }
    context
        .temp_dir
        .child("constraints.txt")
        .write_str(&format!(
            "idna==3.3{alternatives} --hash=sha384:{constraint_hash}\n"
        ))?;
    let requests_before = server
        .received_requests()
        .await
        .context("request recording disabled")?
        .iter()
        .filter(|request| request.url.path() == "/sdist/artifact")
        .count();
    context
        .tool_run()
        .args([
            "--locked",
            "--isolated",
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
        .assert()
        .success();
    assert!(marker.path().exists());
    let requests_after = server
        .received_requests()
        .await
        .context("request recording disabled")?
        .iter()
        .filter(|request| request.url.path() == "/sdist/artifact")
        .count();
    assert_eq!(requests_after - requests_before, 1);
    fs_err::remove_file(marker.path())?;
    context
        .temp_dir
        .child("constraints.txt")
        .write_str(&format!(
            "idna==3.3 --hash=sha384:{constraint_hash} --hash=md5:{}\n",
            "0".repeat(32)
        ))?;
    context
        .tool_run()
        .args([
            "--locked",
            "--isolated",
            "--offline",
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
        .assert()
        .success();
    assert!(!marker.path().exists());
    Ok(())
}

#[tokio::test]
async fn packaged_lock_checks_all_artifact_hashes() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_tool_dirs();
    context.temp_dir.child("wheels").create_dir_all()?;
    let marker = context.temp_dir.child("backend-executed");
    let source =
        generate_source_archive(&"idna".parse()?, &"3.3".parse()?, "", Some(marker.path()))?;
    let source_hash = hex::encode(Sha256::digest(&source));
    let (wheel_filename, wheel) = generate_wheel_with_files(
        &"idna".parse()?,
        &"3.3".parse()?,
        &[],
        &BTreeMap::new(),
        None,
        "py3-none-any",
        &[],
    );
    let wheel_hash = hex::encode(Sha256::digest(&wheel));
    let server = MockServer::start().await;
    let (source_index, source_url) = mount_locked_artifact(
        &server,
        "sdist",
        "idna-3.3.tar.gz",
        &source,
        &source_hash,
        None,
    )
    .await;
    let (wheel_index, wheel_url) =
        mount_locked_artifact(&server, "wheel", &wheel_filename, &wheel, &wheel_hash, None).await;
    let bad_hash = "0".repeat(128);
    let source_lock = locked_artifact(
        &source_index,
        &source_url,
        "idna-3.3.tar.gz",
        &format!("sha256 = \"{source_hash}\", sha512 = \"{bad_hash}\""),
        "sdist",
    );
    let wheel_lock = locked_artifact(
        &wheel_index,
        &wheel_url,
        &wheel_filename,
        &format!("sha256 = \"{wheel_hash}\", sha512 = \"{bad_hash}\""),
        "wheels",
    );
    tool(&context, "1.0.0", Some(&source_lock), None)?;
    tool(&context, "2.0.0", Some(&wheel_lock), None)?;
    context
        .temp_dir
        .child("overrides.txt")
        .write_str("unused==1.0\n")?;
    for (version, index, modified) in [
        ("1.0.0", &source_index, false),
        ("1.0.0", &source_index, true),
        ("2.0.0", &wheel_index, false),
        ("2.0.0", &wheel_index, true),
    ] {
        let mut command = context.tool_install();
        command.args([
            "--locked",
            "--preview-features",
            "locked-tools",
            "--index-url",
            index,
            "--find-links",
            "wheels",
            "--from",
            &format!("locked-tool=={version}"),
            "locked-tool",
        ]);
        if modified {
            command.args(["--override", "overrides.txt"]);
        }
        let output = command
            .env_remove(EnvVars::UV_EXCLUDE_NEWER)
            .env(EnvVars::PATH, context.temp_dir.child("bin").path())
            .output()?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success()
                && (stderr.contains("Hash mismatch")
                    || stderr.contains("does not match the required hashes")),
            "{version} (modified: {modified}): {stderr}"
        );
        assert!(!marker.path().exists());
    }
    tool(
        &context,
        "3.0.0",
        Some(&source_lock.replace(&bad_hash, &hex::encode(Sha512::digest(&source)))),
        None,
    )?;
    tool(
        &context,
        "4.0.0",
        Some(&wheel_lock.replace(&bad_hash, &hex::encode(Sha512::digest(&wheel)))),
        None,
    )?;
    for (version, index) in [("3.0.0", &source_index), ("4.0.0", &wheel_index)] {
        context
            .tool_run()
            .args([
                "--locked",
                "--isolated",
                "--preview-features",
                "locked-tools",
                "--index-url",
                index,
                "--find-links",
                "wheels",
                "--from",
                &format!("locked-tool=={version}"),
                "locked-tool",
            ])
            .env_remove(EnvVars::UV_EXCLUDE_NEWER)
            .assert()
            .success();
    }
    Ok(())
}

#[tokio::test]
async fn packaged_lock_checks_orphaned_source_url_constraint_before_building() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_tool_dirs();
    context.temp_dir.child("wheels").create_dir_all()?;
    let marker = context.temp_dir.child("backend-executed");
    let source =
        generate_source_archive(&"idna".parse()?, &"3.3".parse()?, "", Some(marker.path()))?;
    let hash = hex::encode(Sha256::digest(&source));
    let server = MockServer::start().await;
    let index = format!("{}/simple", server.uri());
    let url = format!("{}/files/idna-3.3.tar.gz", server.uri());
    Mock::given(method("GET"))
        .and(path("/simple/idna/"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(
                json!({"meta": {"api-version": "1.1"}, "name": "idna", "files": [{
                    "filename": "idna-3.3.tar.gz", "url": url, "hashes": {"sha256": hash},
                    "upload-time": "2023-01-01T00:00:00Z",
                }]})
                .to_string(),
                "application/vnd.pypi.simple.v1+json",
            ),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/idna-3.3.tar.gz"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(source))
        .mount(&server)
        .await;
    let lock = locked_artifact(
        &index,
        &url,
        "idna-3.3.tar.gz",
        &format!("sha256 = \"{hash}\""),
        "sdist",
    );
    let (filename, bytes) = generate_wheel_with_files(
        &"locked-tool".parse()?,
        &"1.0.0".parse()?,
        &[],
        &BTreeMap::new(),
        None,
        "py3-none-any",
        &[
            ("locked_tool-1.0.0.dist-info/pylock.toml", &lock),
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
    context
        .temp_dir
        .child("overrides.txt")
        .write_str("unused==1.0\n")?;
    context
        .temp_dir
        .child("constraints.txt")
        .write_str(&format!("idna @ {url}#sha512={}\n", "0".repeat(128)))?;
    let output = context
        .tool_install()
        .args([
            "--locked",
            "--preview-features",
            "locked-tools",
            "--index-url",
            &index,
            "--find-links",
            "wheels",
            "--override",
            "overrides.txt",
            "--constraint",
            "constraints.txt",
            "locked-tool",
        ])
        .env(EnvVars::PATH, context.temp_dir.child("bin").path())
        .output()?;
    assert!(!output.status.success());
    assert!(
        !marker.path().exists(),
        "the backend ran before the constraint hash was checked: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Hash mismatch"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    context
        .temp_dir
        .child("constraints.txt")
        .write_str(&format!(
            "idna @ {url} --hash=sha256:{hash} --hash=sha512:{}\n",
            "0".repeat(128)
        ))?;
    let output = context
        .tool_install()
        .args([
            "--locked",
            "--preview-features",
            "locked-tools",
            "--index-url",
            &index,
            "--find-links",
            "wheels",
            "--override",
            "overrides.txt",
            "--constraint",
            "constraints.txt",
            "locked-tool",
        ])
        .env(EnvVars::PATH, context.temp_dir.child("bin").path())
        .output()?;
    assert!(!output.status.success());
    assert!(
        !marker.path().exists(),
        "the backend ran before all direct URL hashes were checked: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Hash mismatch"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[tokio::test]
async fn packaged_lock_checks_artifact_url_hash_before_building() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_tool_dirs();
    context.temp_dir.child("wheels").create_dir_all()?;
    let marker = context.temp_dir.child("backend-executed");
    let sdist =
        generate_source_archive(&"idna".parse()?, &"3.3".parse()?, "", Some(marker.path()))?;
    let hash = hex::encode(Sha256::digest(&sdist));
    let lock_hash = hex::encode(Sha512::digest(&sdist));
    let context = context
        .with_filter((hash.clone(), "[INDEX_HASH]"))
        .with_filter((lock_hash.clone(), "[LOCK_HASH]"));
    let server = MockServer::start().await;
    let filename = "idna-3.3.tar.gz";
    let (index, url) = mount_locked_artifact(&server, "sdist", filename, &sdist, &hash, None).await;
    let lock = locked_artifact(
        &index,
        &format!("{url}#sha512={}", "0".repeat(128)),
        filename,
        &format!("sha512 = \"{lock_hash}\""),
        "sdist",
    );
    tool(&context, "1.0.0", Some(&lock), None)?;
    uv_snapshot!(context.filters(), context.tool_run().args(["--locked", "--isolated", "--preview-features", "locked-tools", "--find-links", "wheels", "--index-url", &index, "locked-tool"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Hash mismatch for `idna==3.3`

    Expected:
      sha512:[LOCK_HASH]
      sha512:00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
      sha256:[INDEX_HASH]

    Computed:
      sha256:[INDEX_HASH]
      sha512:[LOCK_HASH]
    ");
    assert!(!marker.path().exists());
    Ok(())
}

#[tokio::test]
async fn packaged_lock_checks_locked_source_reference_before_building() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_tool_dirs();
    context.temp_dir.child("wheels").create_dir_all()?;
    let marker = context.temp_dir.child("backend-executed");
    let source =
        generate_source_archive(&"idna".parse()?, &"3.3".parse()?, "", Some(marker.path()))?;
    let hash = hex::encode(Sha256::digest(&source));
    let server = MockServer::start().await;
    let index = format!("{}/simple", server.uri());
    let source_url = format!("{}/files/idna-3.3.tar.gz", server.uri());
    Mock::given(method("GET"))
        .and(path("/simple/idna/"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(
                json!({"meta": {"api-version": "1.1"}, "name": "idna", "files": [{
                    "filename": "idna-3.3.tar.gz",
                    "url": source_url,
                    "hashes": {"sha256": hash},
                    "upload-time": "2023-01-01T00:00:00Z",
                }]})
                .to_string(),
                "application/vnd.pypi.simple.v1+json",
            ),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/idna-3.3.tar.gz"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(source))
        .mount(&server)
        .await;
    let mut lock = locked_artifact(
        &index,
        &source_url,
        "idna-3.3.tar.gz",
        &format!("sha256 = \"{hash}\""),
        "sdist",
    );
    writeln!(
        lock,
        "[[packages]]\nname = \"beta\"\nversion = \"1.0\"\nindex = \"https://unconfigured.example/simple\"\nwheels = [{{ url = \"https://unconfigured.example/beta-1.0-py3-none-any.whl\", hashes = {{ sha256 = \"{}\" }} }}]",
        "0".repeat(64)
    )?;
    tool(&context, "1.0.0", Some(&lock), None)?;
    let (filename, bytes) = generate_wheel_with_files(
        &"beta".parse()?,
        &"2.0".parse()?,
        &[format!("idna @ {source_url}#sha512={}", "0".repeat(128)).parse()?],
        &BTreeMap::new(),
        None,
        "py3-none-any",
        &[],
    );
    let wheel = context.temp_dir.child("wheels").child(filename);
    wheel.write_binary(&bytes)?;
    let wheel_url = url::Url::from_file_path(wheel.path())
        .map_err(|()| anyhow::anyhow!("Could not create a file URL"))?;
    context
        .temp_dir
        .child("overrides.txt")
        .write_str(&format!("beta @ {wheel_url}\n"))?;
    let output = context
        .tool_install()
        .args([
            "--locked",
            "--preview-features",
            "locked-tools",
            "--index-url",
            &index,
            "--find-links",
            "wheels",
            "--override",
            "overrides.txt",
            "locked-tool",
        ])
        .env(EnvVars::PATH, context.temp_dir.child("bin").path())
        .output()?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Hash mismatch"));
    assert!(!marker.path().exists());
    Ok(())
}

#[tokio::test]
async fn packaged_lock_checks_repeated_source_hashes_before_building() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filtered_counts()
        .with_tool_dirs();
    context.temp_dir.child("wheels").create_dir_all()?;
    let marker = context.temp_dir.child("backend-executed");
    let source =
        generate_source_archive(&"beta".parse()?, &"1.0".parse()?, "", Some(marker.path()))?;
    let hash = hex::encode(Sha256::digest(&source));
    let context = context.with_filter((hash.clone(), "[SOURCE_HASH]"));
    let server = MockServer::start().await;
    let source_url = format!("{}/beta-1.0.tar.gz", server.uri());
    Mock::given(method("GET"))
        .and(path("/beta-1.0.tar.gz"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(source))
        .mount(&server)
        .await;
    let lock = locked_artifact(
        "https://unconfigured.example/simple",
        "https://unconfigured.example/idna-3.3-py3-none-any.whl",
        "idna-3.3-py3-none-any.whl",
        &format!("sha256 = \"{}\"", "0".repeat(64)),
        "wheels",
    );
    tool(&context, "1.0.0", Some(&lock), None)?;
    let valid = format!("beta @ {source_url}#sha256={hash}");
    let invalid = format!("beta @ {source_url}#sha512={}", "0".repeat(128));
    let inactive = format!(
        "beta @ {source_url}#sha256={} ; python_version < '2.0'",
        "0".repeat(64)
    );
    for (version, requirements, metadata, success) in [
        ("3.5", vec![&valid, &invalid], false, false),
        // Supplied metadata must not bypass validation when building the source.
        ("3.6", vec![&invalid, &valid], true, false),
        // An inactive reference must not constrain the active source.
        ("3.7", vec![&valid, &inactive], false, true),
    ] {
        let dependencies = requirements
            .into_iter()
            .map(|requirement| requirement.parse())
            .collect::<Result<Vec<_>, _>>()?;
        let (filename, bytes) = generate_wheel_with_files(
            &"idna".parse()?,
            &version.parse()?,
            &dependencies,
            &BTreeMap::new(),
            None,
            "py3-none-any",
            &[],
        );
        let wheel = context.temp_dir.child("wheels").child(filename);
        wheel.write_binary(&bytes)?;
        let wheel_url = url::Url::from_file_path(wheel.path())
            .map_err(|()| anyhow::anyhow!("Could not create a file URL"))?;
        context
            .temp_dir
            .child("overrides.txt")
            .write_str(&format!("idna @ {wheel_url}\n"))?;
        let mut command = context.tool_install();
        command.args([
            "--locked",
            "--preview-features",
            "locked-tools",
            "--find-links",
            "wheels",
            "--override",
            "overrides.txt",
            "locked-tool",
        ]);
        if metadata {
            context.temp_dir.child("uv.toml").write_str(
                "dependency-metadata = [{ name = \"beta\", version = \"1.0\", requires-dist = [] }]\n",
            )?;
            command.args(["--config-file", "uv.toml"]);
        } else if success {
            command.arg("--no-config");
        }
        let output = command
            .env(EnvVars::PATH, context.temp_dir.child("bin").path())
            .output()?;
        assert_eq!(
            output.status.success(),
            success,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if !success {
            assert!(String::from_utf8_lossy(&output.stderr).contains("Hash mismatch"));
        }
        assert_eq!(marker.path().exists(), success);
    }
    Ok(())
}

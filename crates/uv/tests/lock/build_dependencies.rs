use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::time::{Duration, Instant};

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::*;
use sha2::{Digest, Sha256};
use uv_lock::{BuildOperation, BuildStage, Lock};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_test::packse::{generate_sdist_with_files, generate_wheel_with_files};
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

const BACKEND: &str = r#"
import importlib.util
import tomllib
from pathlib import Path
from zipfile import ZipFile

def get_requires_for_build_wheel(config_settings=None):
    assert importlib.util.find_spec("dynamic_helper") is None
    assert importlib.util.find_spec("runtime_only") is None
    return ["dynamic-helper==1.0.0"]

get_requires_for_build_editable = get_requires_for_build_wheel

def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    import helper
    import dynamic_helper
    assert importlib.util.find_spec("runtime_only") is None
    project = tomllib.loads(Path("pyproject.toml").read_text())["project"]
    name = project["name"].replace("-", "_")
    version = project["version"]
    dist_info = f"{name}-{version}.dist-info"
    filename = f"{name}-{version}-py3-none-any.whl"
    metadata = f"Metadata-Version: 2.3\nName: {name}\nVersion: {version}\n"
    metadata += "".join(f"Requires-Dist: {req}\n" for req in project.get("dependencies", []))
    with ZipFile(Path(wheel_directory) / filename, "w") as wheel:
        wheel.writestr(f"{name}.py", f"selected = {helper.__version__!r}\n")
        wheel.writestr(f"{dist_info}/METADATA", metadata)
        wheel.writestr(f"{dist_info}/WHEEL", "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n")
        wheel.writestr(f"{dist_info}/RECORD", "")
    return filename

build_editable = build_wheel
"#;

fn write_wheel(
    root: &Path,
    name: &str,
    version: &str,
    requires: &[&str],
    files: &[(&str, &str)],
) -> Result<PathBuf> {
    let requirements = requires
        .iter()
        .map(|value| uv_pep508::Requirement::from_str(value))
        .collect::<Result<Vec<_>, _>>()?;
    let (filename, bytes) = generate_wheel_with_files(
        &PackageName::from_str(name)?,
        &Version::from_str(version)?,
        &requirements,
        &BTreeMap::default(),
        None,
        "py3-none-any",
        files,
    );
    let path = root.join(filename);
    fs_err::write(&path, bytes)?;
    Ok(path)
}

fn fixtures(root: &Path) -> Result<PathBuf> {
    let files = root.join("files");
    fs_err::create_dir_all(&files)?;
    write_wheel(&files, "helper", "1.0.0", &[], &[])?;
    write_wheel(&files, "helper", "2.0.0", &[], &[])?;
    write_wheel(&files, "dynamic-helper", "1.0.0", &[], &[])?;
    write_wheel(&files, "runtime-only", "1.0.0", &[], &[])?;
    write_wheel(
        &files,
        "builder",
        "1.0.0",
        &["helper>=1,<3"],
        &[("builder/backend.py", BACKEND)],
    )?;
    Ok(files)
}

fn source(root: &Path, name: &str, helper: &str) -> Result<()> {
    fs_err::create_dir_all(root)?;
    fs_err::write(
        root.join("pyproject.toml"),
        format!(
            r#"
[project]
name = "{name}"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = ["runtime-only==1.0.0"]

[build-system]
requires = ["builder==1.0.0", "helper=={helper}"]
build-backend = "builder.backend"
"#
        ),
    )?;
    Ok(())
}

fn workspace(context: &uv_test::TestContext) -> Result<PathBuf> {
    let files = fixtures(context.temp_dir.path())?;
    context.temp_dir.child("pyproject.toml").write_str(
        r#"
[project]
name = "project"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = ["first", "second"]

[tool.uv.sources]
first = { path = "first" }
second = { path = "second" }
"#,
    )?;
    source(&context.temp_dir.join("first"), "first", "1.0.0")?;
    source(&context.temp_dir.join("second"), "second", "2.0.0")?;
    Ok(files)
}

fn capture_command(context: &uv_test::TestContext, files: &Path) -> Command {
    let mut command = context.lock();
    command
        .args([
            "--build-dependencies",
            "--preview-features",
            "build-dependency-locking",
            "--offline",
            "--no-index",
            "--find-links",
        ])
        .arg(files);
    command
}

fn capture(context: &uv_test::TestContext, files: &Path) {
    capture_command(context, files).assert().success();
}

#[test]
fn build_dependencies_independent_graphs_replay_and_invalidation() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let files = workspace(&context)?;
    context
        .lock()
        .args(["--preview", "--offline", "--no-index", "--find-links"])
        .arg(&files)
        .assert()
        .success();
    let lockfile = context.temp_dir.child("uv.lock");
    let ordinary = fs_err::read_to_string(&lockfile)?;
    assert!(ordinary.starts_with("version = 1\n"));
    context
        .lock()
        .arg("--build-dependencies")
        .assert()
        .failure();
    assert_eq!(fs_err::read_to_string(&lockfile)?, ordinary);

    capture(&context, &files);
    let encoded = fs_err::read_to_string(&lockfile)?;
    assert!(encoded.starts_with("version = 2\n"));
    let lock = Lock::from_toml(&encoded)?;
    let builds = lock.build_lock().expect("captured build lock");
    assert_eq!(builds.resolutions().len(), 4);
    let mut selected = Vec::new();
    for build in builds.resolutions() {
        let graph = build.graph(BuildStage::Final).expect("final graph");
        let helper = graph
            .packages()
            .iter()
            .find(|package| package.name().as_ref() == "helper")
            .and_then(|package| package.version())
            .expect("helper version");
        selected.push(format!(
            "{} {} {helper}",
            build.source().name(),
            build.operation()
        ));
        assert!(
            graph
                .packages()
                .iter()
                .all(|package| package.name().as_ref() != "runtime-only")
        );
    }
    insta::assert_debug_snapshot!(selected, @r#"
    [
        "first wheel 1.0.0",
        "first editable 1.0.0",
        "second wheel 2.0.0",
        "second editable 2.0.0",
    ]
    "#);

    // The wheel files remain available, but no candidate index or old build cache is available.
    let context = context.with_cache_dir("fresh-cache");
    context
        .sync()
        .args(["--frozen", "--offline", "--no-index", "--no-editable"])
        .assert()
        .success();
    context.run().args(["--no-sync", "python", "-c", "import first, second, importlib.util; assert first.selected == '1.0.0'; assert second.selected == '2.0.0'; assert importlib.util.find_spec('builder') is None"]).assert().success();
    let receipt = context
        .site_packages()
        .join("first-0.1.0.dist-info/uv_build.json");
    let receipt: uv_distribution_types::BuildInfo =
        serde_json::from_str(&fs_err::read_to_string(&receipt)?)?;
    assert_eq!(
        receipt.build_lock_fingerprint(),
        Some(&builds.fingerprint()?)
    );

    // A changed declaration is rejected even if both the installed wheel and cache are reusable.
    source(&context.temp_dir.join("first"), "first", "2.0.0")?;
    context
        .sync()
        .args(["--frozen", "--offline", "--no-index", "--no-editable"])
        .assert()
        .failure();
    capture(&context, &files);
    assert_ne!(fs_err::read_to_string(&lockfile)?, encoded);
    context
        .sync()
        .args(["--frozen", "--offline", "--no-index", "--no-editable"])
        .assert()
        .success();
    context
        .run()
        .args([
            "--no-sync",
            "python",
            "-c",
            "import first; assert first.selected == '2.0.0'",
        ])
        .assert()
        .success();

    context
        .lock()
        .args([
            "--no-build-dependencies",
            "--offline",
            "--no-index",
            "--find-links",
        ])
        .arg(&files)
        .assert()
        .success();
    assert!(fs_err::read_to_string(&lockfile)?.starts_with("version = 1\n"));
    Ok(())
}

#[test]
fn build_dependencies_reconcile_final_environment() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let files = fixtures(context.temp_dir.path())?;
    write_wheel(&files, "bootstrap-only", "1.0.0", &[], &[])?;
    write_wheel(&files, "helper", "2.0.0", &["bootstrap-only==1.0.0"], &[])?;
    let backend = BACKEND.replace(
        "return [\"dynamic-helper==1.0.0\"]",
        "assert importlib.util.find_spec(\"bootstrap_only\") is not None\n    return [\"dynamic-helper==1.0.0\", \"helper==1.0.0\"]",
    ).replace("    import helper\n", "    assert importlib.util.find_spec(\"bootstrap_only\") is None\n    import helper\n");
    write_wheel(
        &files,
        "switching-builder",
        "1.0.0",
        &["helper>=1,<3"],
        &[("switching_builder/backend.py", &backend)],
    )?;
    context.temp_dir.child("pyproject.toml").write_str(
        r#"
[project]
name = "switching"
version = "0.1.0"
requires-python = ">=3.12"

[build-system]
requires = ["switching-builder==1.0.0"]
build-backend = "switching_builder.backend"
"#,
    )?;
    capture(&context, &files);
    let lock = Lock::from_toml(&fs_err::read_to_string(context.temp_dir.child("uv.lock"))?)?;
    let build = lock
        .build_lock()
        .expect("build lock")
        .resolutions()
        .iter()
        .find(|build| build.operation() == BuildOperation::Wheel)
        .expect("wheel build");
    assert!(
        build
            .graph(BuildStage::Bootstrap)
            .expect("bootstrap")
            .packages()
            .iter()
            .any(|package| package.name().as_ref() == "bootstrap-only")
    );
    assert!(
        build
            .graph(BuildStage::Final)
            .expect("final")
            .packages()
            .iter()
            .all(|package| package.name().as_ref() != "bootstrap-only")
    );
    let context = context.with_cache_dir("fresh-cache");
    context
        .sync()
        .args(["--frozen", "--offline", "--no-index", "--no-editable"])
        .assert()
        .success();
    context
        .run()
        .args([
            "--no-sync",
            "python",
            "-c",
            "import switching; assert switching.selected == '1.0.0'",
        ])
        .assert()
        .success();
    Ok(())
}

#[test]
fn build_dependencies_missing_coverage_fails_before_reuse() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let files = workspace(&context)?;
    capture(&context, &files);
    context
        .sync()
        .args(["--frozen", "--offline", "--no-index", "--no-editable"])
        .assert()
        .success();
    let lockfile = context.temp_dir.child("uv.lock");
    let encoded = fs_err::read_to_string(&lockfile)?;

    // Installed distributions are already current, but source coverage is still mandatory.
    let mut missing = toml::from_str::<toml::Value>(&encoded)?;
    missing["build-lock"]["resolution"]
        .as_array_mut()
        .expect("build resolutions")
        .retain(|resolution| resolution["name"].as_str() != Some("first"));
    fs_err::write(&lockfile, toml::to_string(&missing)?)?;
    let output = context
        .sync()
        .args(["--frozen", "--offline", "--no-index", "--no-editable"])
        .output()?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("does not cover the wheel build"));

    let mut other_executor = toml::from_str::<toml::Value>(&encoded)?;
    other_executor["build-lock"]["executor"]["abi-tag"] =
        toml::Value::String("different".to_owned());
    fs_err::write(&lockfile, toml::to_string(&other_executor)?)?;
    let output = context
        .sync()
        .args(["--frozen", "--offline", "--no-index", "--no-editable"])
        .output()?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("does not cover this executor"));

    fs_err::write(&lockfile, encoded)?;
    context
        .sync()
        .args([
            "--frozen",
            "--offline",
            "--no-index",
            "--no-editable",
            "--no-build-isolation",
        ])
        .assert()
        .failure();
    context
        .export()
        .args(["--frozen", "--offline"])
        .assert()
        .failure();
    for flags in [
        vec!["--frozen"],
        vec!["--no-sync"],
        vec!["--no-sync", "--isolated"],
    ] {
        let output = context
            .run()
            .args(flags)
            .args(["--with", "runtime-only==1.0.0", "python", "-c", "pass"])
            .output()?;
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("ephemeral `--with`"));
    }
    Ok(())
}

#[test]
fn build_dependencies_replaced_wheel_fails_hash_verification() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let files = workspace(&context)?;
    capture(&context, &files);
    let lockfile = context.temp_dir.child("uv.lock");
    let encoded = fs_err::read_to_string(&lockfile)?;
    write_wheel(
        &files,
        "builder",
        "1.0.0",
        &["helper>=1,<3"],
        &[(
            "builder/backend.py",
            "raise RuntimeError('replaced backend executed')",
        )],
    )?;
    let context = context.with_cache_dir("fresh-cache");
    let output = context
        .sync()
        .args(["--frozen", "--offline", "--no-index", "--no-editable"])
        .output()?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Hash mismatch"), "{stderr}");
    assert!(!stderr.contains("replaced backend executed"), "{stderr}");
    assert_eq!(fs_err::read_to_string(lockfile)?, encoded);
    Ok(())
}

#[test]
fn build_dependencies_build_wheel() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let files = fixtures(context.temp_dir.path())?;
    source(context.temp_dir.path(), "project", "1.0.0")?;
    capture(&context, &files);
    let lockfile = context.temp_dir.child("uv.lock");
    let encoded = fs_err::read_to_string(&lockfile)?;

    // Neither build dependencies nor a populated build cache are required at replay time.
    let context = context.with_cache_dir("fresh-build-cache");
    context
        .build()
        .args(["--wheel", "--offline", "--no-index"])
        .assert()
        .success();
    let wheel = context
        .temp_dir
        .child("dist/project-0.1.0-py3-none-any.whl");
    let built = fs_err::read(&wheel)?;
    assert!(!built.is_empty());

    for args in [
        vec!["--clear"],
        vec!["--sdist", "--clear"],
        vec!["--wheel", "--list", "--clear"],
        vec!["--wheel", "--no-build-isolation", "--clear"],
    ] {
        context.build().args(args).assert().failure();
        assert_eq!(fs_err::read(&wheel)?, built);
    }

    source(context.temp_dir.path(), "project", "2.0.0")?;
    let output = context
        .build()
        .args(["--wheel", "--offline", "--no-index", "--clear"])
        .output()?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("build declarations"));
    assert_eq!(fs_err::read(&wheel)?, built);
    fs_err::remove_file(context.temp_dir.join("pyproject.toml"))?;
    context
        .temp_dir
        .child("setup.py")
        .write_str("raise RuntimeError('unlocked legacy build')\n")?;
    let output = context
        .build()
        .args(["--wheel", "--offline", "--no-index", "--clear"])
        .output()?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("required build lock"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("unlocked legacy build"));
    assert_eq!(fs_err::read(&wheel)?, built);
    assert_eq!(fs_err::read_to_string(lockfile)?, encoded);
    Ok(())
}

#[test]
fn build_dependencies_upgrade_constraints_and_preferences() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let files = fixtures(context.temp_dir.path())?;
    source(context.temp_dir.path(), "project", "1.0.0")?;
    let pyproject = context.temp_dir.child("pyproject.toml");
    let contents = fs_err::read_to_string(&pyproject)?;
    fs_err::write(
        &pyproject,
        contents.replace("helper==1.0.0", "helper>=1,<3"),
    )?;
    capture_command(&context, &files)
        .args(["--upgrade-package", "helper==1.0.0"])
        .assert()
        .success();
    let lockfile = context.temp_dir.child("uv.lock");
    let first = fs_err::read_to_string(&lockfile)?;
    capture(&context, &files);
    assert_eq!(fs_err::read_to_string(&lockfile)?, first);

    context
        .lock()
        .args([
            "--upgrade-package",
            "helper==2.0.0",
            "--offline",
            "--no-index",
            "--find-links",
        ])
        .arg(&files)
        .assert()
        .success();
    let second = fs_err::read_to_string(&lockfile)?;
    let lock = Lock::from_toml(&second)?;
    for build in lock.build_lock().expect("build contract").resolutions() {
        let graph = build.graph(BuildStage::Final).expect("final graph");
        assert_eq!(
            graph
                .packages()
                .iter()
                .find(|package| package.name().as_ref() == "helper")
                .and_then(|package| package.version())
                .map(ToString::to_string)
                .as_deref(),
            Some("2.0.0")
        );
    }
    context
        .lock()
        .args([
            "--upgrade-package",
            "helper==3.0.0",
            "--offline",
            "--no-index",
            "--find-links",
        ])
        .arg(&files)
        .assert()
        .failure();
    assert_eq!(fs_err::read_to_string(&lockfile)?, second);
    Ok(())
}

#[test]
fn build_dependencies_reject_nested_source_without_publishing() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let files = fixtures(context.temp_dir.path())?;
    source(context.temp_dir.path(), "project", "1.0.0")?;
    write_wheel(
        &files,
        "builder",
        "1.0.0",
        &["helper>=1,<3", "nested==1.0.0"],
        &[("builder/backend.py", BACKEND)],
    )?;
    let (filename, archive) = generate_sdist_with_files(
        &PackageName::from_str("nested")?,
        &Version::from_str("1.0.0")?,
        &[
            (
                "pyproject.toml",
                "[build-system]\nrequires = []\nbuild-backend = 'backend'\nbackend-path = ['.']\n",
            ),
            (
                "backend.py",
                "raise RuntimeError('nested source executed')\n",
            ),
            (
                "PKG-INFO",
                "Metadata-Version: 2.3\nName: nested\nVersion: 1.0.0\n",
            ),
        ],
    );
    fs_err::write(files.join(filename), archive)?;
    context
        .lock()
        .args(["--offline", "--no-index", "--find-links"])
        .arg(&files)
        .assert()
        .success();
    let lockfile = context.temp_dir.child("uv.lock");
    let original = fs_err::read_to_string(&lockfile)?;
    let output = capture_command(&context, &files).output()?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("nested source executed"), "{stderr}");
    assert!(
        stderr.contains("no usable wheels") || stderr.contains("source builds"),
        "{stderr}"
    );
    assert_eq!(fs_err::read_to_string(lockfile)?, original);
    Ok(())
}

#[test]
fn build_dependencies_reject_dynamic_runtime_metadata() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let files = fixtures(context.temp_dir.path())?;
    source(context.temp_dir.path(), "project", "1.0.0")?;
    let pyproject = context.temp_dir.child("pyproject.toml");
    let contents = fs_err::read_to_string(&pyproject)?;
    fs_err::write(
        &pyproject,
        contents.replace(
            "dependencies = [\"runtime-only==1.0.0\"]",
            "dynamic = [\"dependencies\"]",
        ),
    )?;
    context
        .lock()
        .args(["--offline", "--no-index", "--find-links"])
        .arg(&files)
        .assert()
        .success();
    let lockfile = context.temp_dir.child("uv.lock");
    let original = fs_err::read_to_string(&lockfile)?;
    let output = capture_command(&context, &files).output()?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("requires static source metadata"),
        "{stderr}"
    );
    assert_eq!(fs_err::read_to_string(lockfile)?, original);
    Ok(())
}

#[test]
fn build_dependencies_reject_script_without_mutation() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let script = context.temp_dir.child("example.py");
    script.write_str("print('hello')\n")?;
    context
        .lock()
        .args([
            "--script",
            "example.py",
            "--build-dependencies",
            "--preview-features",
            "build-dependency-locking",
        ])
        .assert()
        .failure();
    assert_eq!(fs_err::read_to_string(script)?, "print('hello')\n");
    assert!(!context.temp_dir.join("example.py.lock").exists());
    Ok(())
}

#[test]
fn build_dependencies_reject_tool_installation() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_tool_dirs();
    let files = workspace(&context)?;
    capture(&context, &files);
    let encoded = fs_err::read_to_string(context.temp_dir.join("uv.lock"))?;
    let tool_dir = context.temp_dir.child("tools/helper");
    tool_dir.create_dir_all()?;
    let tool_lock = tool_dir.child("uv.lock");
    tool_lock.write_str(&encoded)?;

    for preview in [false, true] {
        let mut install = context.tool_install();
        install
            .args(["helper==1.0.0", "--offline", "--no-index", "--find-links"])
            .arg(&files)
            .arg("--force");
        if preview {
            install.args(["--preview-features", "tool-install-locks"]);
        }
        install.assert().failure().stderr(predicates::str::contains(
            "requires build dependency locking, which tool installation does not support",
        ));

        let mut upgrade = context.tool_upgrade();
        upgrade.arg("helper");
        if preview {
            upgrade.args(["--preview-features", "tool-install-locks"]);
        }
        upgrade.assert().failure().stderr(predicates::str::contains(
            "requires build dependency locking, which tool installation does not support",
        ));
        assert_eq!(fs_err::read_to_string(&tool_lock)?, encoded);
    }
    Ok(())
}

#[cfg(feature = "test-git")]
#[test]
fn build_dependencies_path_archive_and_git_source() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let files = fixtures(context.temp_dir.path())?;
    let tree = context.temp_dir.join("archive-tree");
    source(&tree, "archive", "1.0.0")?;
    let pyproject = fs_err::read_to_string(tree.join("pyproject.toml"))?;
    let (filename, archive) = generate_sdist_with_files(
        &PackageName::from_str("archive")?,
        &Version::from_str("0.1.0")?,
        &[("pyproject.toml", &pyproject)],
    );
    fs_err::write(context.temp_dir.join(&filename), archive)?;

    let repository = context.temp_dir.join("repository");
    source(&repository.join("package"), "git-source", "2.0.0")?;
    Command::new("git")
        .arg("init")
        .arg(&repository)
        .assert()
        .success();
    Command::new("git")
        .arg("-C")
        .arg(&repository)
        .args(["add", "."])
        .assert()
        .success();
    Command::new("git")
        .arg("-C")
        .arg(&repository)
        .args([
            "-c",
            "user.name=Example",
            "-c",
            "user.email=example@example.com",
            "commit",
            "-m",
            "Initial commit",
        ])
        .env("GIT_AUTHOR_DATE", "2000-01-01T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2000-01-01T00:00:00Z")
        .assert()
        .success();
    let repository_url = url::Url::from_directory_path(&repository)
        .map_err(|()| anyhow::anyhow!("invalid repository path"))?;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&format!(
            r#"
[project]
name = "project"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = ["archive", "git-source"]

[tool.uv.sources]
archive = {{ path = "{filename}" }}
git-source = {{ git = "{repository_url}", subdirectory = "package" }}
"#
        ))?;
    capture(&context, &files);
    let encoded = fs_err::read_to_string(context.temp_dir.child("uv.lock"))?;
    let lock = Lock::from_toml(&encoded)?;
    assert_eq!(
        lock.build_lock()
            .expect("build contract")
            .resolutions()
            .len(),
        2
    );
    let context = context.with_cache_dir("fresh-git-cache");
    context
        .sync()
        .args(["--frozen", "--offline", "--no-index", "--no-editable"])
        .assert()
        .success();
    context.run().args(["--no-sync", "python", "-c", "import archive, git_source; assert archive.selected == '1.0.0'; assert git_source.selected == '2.0.0'"]).assert().success();
    Ok(())
}

#[tokio::test]
async fn build_dependencies_hashless_source_archive() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let files = fixtures(context.temp_dir.path())?;
    let tree = context.temp_dir.join("archive");
    source(&tree, "archive", "1.0.0")?;
    let pyproject = fs_err::read_to_string(tree.join("pyproject.toml"))?;
    let (filename, archive) = generate_sdist_with_files(
        &PackageName::from_str("archive")?,
        &Version::from_str("0.1.0")?,
        &[
            ("pyproject.toml", &pyproject),
            (
                "PKG-INFO",
                "Metadata-Version: 2.3\nName: archive\nVersion: 0.1.0\nRequires-Python: >=3.12\nRequires-Dist: runtime-only==1.0.0\n",
            ),
        ],
    );
    let hash = format!("sha256:{}", hex::encode(Sha256::digest(&archive)));
    let server = MockServer::start().await;
    let archive_path = format!("/files/{filename}");
    Mock::given(path("/simple/archive/"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(
                serde_json::json!({
                    "meta": {"api-version": "1.0"},
                    "name": "archive",
                    "files": [{"filename": filename, "url": archive_path, "hashes": {}, "upload-time": "2024-01-01T00:00:00Z"}]
                })
                .to_string(),
                "application/vnd.pypi.simple.v1+json",
            ),
        )
        .mount(&server)
        .await;
    Mock::given(path(&archive_path))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(archive))
        .mount(&server)
        .await;
    context.temp_dir.child("pyproject.toml").write_str(
        r#"
[project]
name = "project"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = ["archive==0.1.0"]
"#,
    )?;
    context
        .lock()
        .args([
            "--build-dependencies",
            "--preview-features",
            "build-dependency-locking",
            "--index-url",
        ])
        .arg(format!("{}/simple", server.uri()))
        .arg("--find-links")
        .arg(&files)
        .assert()
        .success();
    let lockfile = context.temp_dir.child("uv.lock");
    let encoded = fs_err::read_to_string(&lockfile)?;
    let value = toml::from_str::<toml::Value>(&encoded)?;
    let package = value["package"]
        .as_array()
        .expect("packages")
        .iter()
        .find(|package| package["name"].as_str() == Some("archive"))
        .expect("source package");
    assert_eq!(package["sdist"]["hash"].as_str(), Some(hash.as_str()));

    // A required contract cannot be read after its source artifact hash is removed.
    let mut missing_hash = value;
    missing_hash["package"]
        .as_array_mut()
        .expect("packages")
        .iter_mut()
        .find(|package| package["name"].as_str() == Some("archive"))
        .expect("source package")["sdist"]
        .as_table_mut()
        .expect("sdist")
        .remove("hash");
    assert!(Lock::from_toml(&toml::to_string(&missing_hash)?).is_err());

    context
        .sync()
        .args(["--frozen", "--no-index"])
        .assert()
        .success();
    let (_, replaced) = generate_sdist_with_files(
        &PackageName::from_str("archive")?,
        &Version::from_str("0.1.0")?,
        &[
            (
                "pyproject.toml",
                "[build-system]\nrequires = []\nbuild-backend = 'backend'\nbackend-path = ['.']\n",
            ),
            (
                "backend.py",
                "raise RuntimeError('replaced source executed')\n",
            ),
        ],
    );
    server.reset().await;
    Mock::given(path(&archive_path))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(replaced))
        .mount(&server)
        .await;
    let context = context.with_cache_dir("replaced-source-cache");
    let output = context
        .sync()
        .args(["--frozen", "--no-index", "--reinstall"])
        .output()?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Hash mismatch"), "{stderr}");
    assert!(!stderr.contains("replaced source executed"), "{stderr}");
    assert_eq!(fs_err::read_to_string(lockfile)?, encoded);
    Ok(())
}

fn wait_for_file(path: &Path) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !path.exists() {
        anyhow::ensure!(
            Instant::now() < deadline,
            "Timed out waiting for {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

#[test]
fn build_dependencies_atomic_capture_and_concurrent_writers() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let files = fixtures(context.temp_dir.path())?;
    source(context.temp_dir.path(), "project", "1.0.0")?;
    let gated = BACKEND.replace(
        "def get_requires_for_build_wheel(config_settings=None):\n",
        r#"def get_requires_for_build_wheel(config_settings=None):
    import os, time
    if gate := os.environ.get("UV_BUILD_LOCK_TEST_GATE"):
        Path(gate + ".ready").touch()
        deadline = time.monotonic() + 45
        while not Path(gate + ".release").exists():
            assert time.monotonic() < deadline, "build lock test gate timed out"
            time.sleep(0.01)
"#,
    );
    write_wheel(
        &files,
        "builder",
        "1.0.0",
        &["helper>=1,<3"],
        &[("builder/backend.py", &gated)],
    )?;
    context
        .lock()
        .args(["--offline", "--no-index", "--find-links"])
        .arg(&files)
        .assert()
        .success();
    let lockfile = context.temp_dir.child("uv.lock");
    let original = fs_err::read_to_string(&lockfile)?;

    // An external edit made during backend discovery must not be overwritten.
    let gate = context.temp_dir.join("external-edit");
    let child = capture_command(&context, &files)
        .env("UV_BUILD_LOCK_TEST_GATE", &gate)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    wait_for_file(&gate.with_extension("ready"))?;
    let edited = format!("{original}\n# concurrent edit\n");
    fs_err::write(&lockfile, &edited)?;
    fs_err::write(gate.with_extension("release"), "")?;
    let output = child.wait_with_output()?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("lockfile changed while resolving"));
    assert_eq!(fs_err::read_to_string(&lockfile)?, edited);

    // Termination leaves the old file intact and releases the writer lock.
    let gate = context.temp_dir.join("interruption");
    let mut child = capture_command(&context, &files)
        .env("UV_BUILD_LOCK_TEST_GATE", &gate)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    wait_for_file(&gate.with_extension("ready"))?;
    child.kill()?;
    fs_err::write(gate.with_extension("release"), "")?;
    assert!(!child.wait()?.success());
    assert_eq!(fs_err::read_to_string(&lockfile)?, edited);
    capture(&context, &files);

    // The removal command must read the result of the preceding capture, even with a separate
    // cache. Its diagnostic provides a deterministic synchronization point for this test.
    let gate = context.temp_dir.join("concurrent-writers");
    let capture = capture_command(&context, &files)
        .env("UV_BUILD_LOCK_TEST_GATE", &gate)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    wait_for_file(&gate.with_extension("ready"))?;
    let other = context.with_cache_dir("concurrent-cache");
    let mut removal = other
        .lock()
        .args([
            "--no-build-dependencies",
            "--offline",
            "--no-index",
            "--find-links",
        ])
        .arg(&files)
        .arg("--verbose")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    let stderr = removal.stderr.take().expect("piped stderr");
    let (send, receive) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut output = String::new();
        for line in BufReader::new(stderr).lines() {
            let line = line.expect("read child stderr");
            if line.contains("Waiting to acquire exclusive lock") && line.contains("uv-lockfile-") {
                let _ = send.send(());
            }
            output.push_str(&line);
            output.push('\n');
        }
        output
    });
    receive.recv_timeout(Duration::from_secs(30))?;
    fs_err::write(gate.with_extension("release"), "")?;
    let output = capture.wait_with_output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let status = removal.wait()?;
    let stderr = reader.join().expect("stderr reader");
    assert!(status.success(), "{stderr}");
    let encoded = fs_err::read_to_string(&lockfile)?;
    assert!(encoded.starts_with("version = 1\n"));
    assert!(Lock::from_toml(&encoded)?.build_lock().is_none());
    Ok(())
}

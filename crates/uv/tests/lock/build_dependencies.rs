use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use assert_fs::prelude::*;
use uv_lock::{BuildOperation, BuildStage, Lock};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_test::packse::generate_wheel_with_files;

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

fn capture(context: &uv_test::TestContext, files: &Path) {
    context
        .lock()
        .args([
            "--build-dependencies",
            "--preview-features",
            "build-dependency-locking",
            "--offline",
            "--no-index",
            "--find-links",
        ])
        .arg(files)
        .assert()
        .success();
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

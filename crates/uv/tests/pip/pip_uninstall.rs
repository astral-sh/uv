#[cfg(unix)]
use std::fmt::Write;
#[cfg(windows)]
use std::path::{Component, Prefix};
#[cfg(unix)]
use std::process::Command;
#[cfg(unix)]
use std::time::{Duration, Instant};

use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::fixture::ChildPath;
use assert_fs::prelude::*;
#[cfg(unix)]
use nix::errno::Errno;
#[cfg(unix)]
use nix::sys::signal::{Signal, kill};
#[cfg(unix)]
use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
#[cfg(unix)]
use nix::unistd::Pid;
#[cfg(unix)]
use url::Url;

use uv_test::uv_snapshot;

#[cfg(unix)]
fn interrupt_when(command: &mut Command, condition: impl Fn() -> bool) -> Result<bool> {
    let mut child = command.spawn()?;
    let pid = Pid::from_raw(i32::try_from(child.id())?);
    let deadline = Instant::now() + Duration::from_secs(30);

    while Instant::now() < deadline && child.try_wait()?.is_none() {
        match kill(pid, Signal::SIGSTOP) {
            Ok(()) => {}
            Err(Errno::ESRCH) => break,
            Err(error) => return Err(error.into()),
        }
        let WaitStatus::Stopped(_, Signal::SIGSTOP) = waitpid(pid, Some(WaitPidFlag::WUNTRACED))?
        else {
            break;
        };

        if condition() {
            child.kill()?;
            child.wait()?;
            return Ok(true);
        }

        if child.try_wait()?.is_some() {
            break;
        }
        kill(pid, Signal::SIGCONT)?;
        std::thread::sleep(Duration::from_micros(100));
    }

    let _ = kill(pid, Signal::SIGCONT);
    let _ = child.kill();
    let _ = child.wait();
    Ok(false)
}

#[test]
fn no_arguments() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.filters(), context.pip_uninstall(), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the following required arguments were not provided:
      <PACKAGE|--requirements <REQUIREMENTS>>

    Usage: uv pip uninstall --cache-dir [CACHE_DIR] <PACKAGE|--requirements <REQUIREMENTS>>

    For more information, try '--help'.
    "
    );
}

#[test]
fn invalid_requirement() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.filters(), context.pip_uninstall()
        .arg("flask==1.0.x"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Failed to parse: `flask==1.0.x`
      Caused by: after parsing `1.0`, found `.x`, which is not part of a valid version
    flask==1.0.x
         ^^^^^^^
    ");
}

#[test]
fn missing_requirements_txt() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.filters(), context.pip_uninstall()
        .arg("-r")
        .arg("requirements.txt"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: File not found: `requirements.txt`
    "
    );
}

#[test]
fn invalid_requirements_txt_requirement() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("flask==1.0.x")?;

    uv_snapshot!(context.filters(), context.pip_uninstall()
        .arg("-r")
        .arg("requirements.txt"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Couldn't parse requirement in `requirements.txt` at position 0
      Caused by: after parsing `1.0`, found `.x`, which is not part of a valid version
    flask==1.0.x
         ^^^^^^^
    ");

    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn uninstall() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    context
        .pip_sync()
        .arg("requirements.txt")
        .assert()
        .success();

    context.assert_command("import markupsafe").success();

    uv_snapshot!(context.pip_uninstall()
        .arg("MarkupSafe"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 1 package in [TIME]
     - markupsafe==2.1.3
    "
    );

    context.assert_command("import markupsafe").failure();

    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn missing_record() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str("MarkupSafe==2.1.3")?;

    context
        .pip_sync()
        .arg("requirements.txt")
        .assert()
        .success();

    context.assert_command("import markupsafe").success();

    // Delete the RECORD file.
    let dist_info = context.site_packages().join("MarkupSafe-2.1.3.dist-info");
    fs_err::remove_file(dist_info.join("RECORD"))?;

    uv_snapshot!(context.filters(), context.pip_uninstall()
        .arg("MarkupSafe"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: Cannot uninstall package; `RECORD` file not found at: [SITE_PACKAGES]/MarkupSafe-2.1.3.dist-info/RECORD
    "
    );

    Ok(())
}

#[test]
#[cfg(unix)]
fn interrupted_uninstall_can_be_retried() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let site_packages = context.site_packages();
    let package = site_packages.join("interrupted");
    let dist_info = site_packages.join("interrupted-1.0.0.dist-info");
    fs_err::create_dir_all(&package)?;
    fs_err::create_dir_all(&dist_info)?;
    fs_err::write(
        dist_info.join("METADATA"),
        "Metadata-Version: 2.1\nName: interrupted\nVersion: 1.0.0\n",
    )?;

    let record_path = dist_info.join("RECORD");
    let mut record = String::from(
        "interrupted-1.0.0.dist-info/METADATA,,\n\
         interrupted-1.0.0.dist-info/RECORD,,\n",
    );
    for index in 0..1_000 {
        let relative_path = format!("interrupted/module_{index:04}.py");
        fs_err::write(site_packages.join(&relative_path), "")?;
        writeln!(record, "{relative_path},,")?;
    }
    fs_err::write(&record_path, record)?;

    let first_payload_file = package.join("module_0000.py");
    let last_payload_file = package.join("module_0999.py");
    let mut command = context.pip_uninstall();
    command.arg("interrupted");
    let interrupted = interrupt_when(&mut command, || {
        record_path.exists() && !first_payload_file.exists() && last_payload_file.exists()
    })?;
    assert!(
        interrupted,
        "failed to interrupt uv while RECORD could support recovery"
    );

    context
        .pip_uninstall()
        .arg("interrupted")
        .assert()
        .success();
    assert!(!package.exists());
    assert!(!dist_info.exists());

    Ok(())
}

#[test]
#[cfg(unix)]
fn interrupted_uninstall_during_directory_cleanup_can_be_retried() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let site_packages = context.site_packages();
    let package = site_packages.join("interrupted_cleanup");
    let bytecode = package.join("__pycache__");
    let dist_info = site_packages.join("interrupted_cleanup-1.0.0.dist-info");
    fs_err::create_dir_all(&bytecode)?;
    fs_err::create_dir_all(&dist_info)?;
    fs_err::write(package.join("__init__.py"), "")?;
    fs_err::write(
        dist_info.join("METADATA"),
        "Metadata-Version: 2.1\nName: interrupted-cleanup\nVersion: 1.0.0\n",
    )?;

    let record_path = dist_info.join("RECORD");
    fs_err::write(
        &record_path,
        "interrupted_cleanup-1.0.0.dist-info/METADATA,,\n\
         interrupted_cleanup-1.0.0.dist-info/RECORD,,\n\
         interrupted_cleanup/__init__.py,,\n",
    )?;
    for index in 0..10_000 {
        fs_err::write(bytecode.join(format!("module_{index:05}.pyc")), "")?;
    }

    let first_bytecode_file = bytecode.join("module_00000.pyc");
    let last_bytecode_file = bytecode.join("module_09999.pyc");
    let mut command = context.pip_uninstall();
    command.arg("interrupted-cleanup");
    let interrupted = interrupt_when(&mut command, || {
        record_path.exists() && first_bytecode_file.exists() != last_bytecode_file.exists()
    })?;
    assert!(
        interrupted,
        "failed to interrupt uv during directory cleanup while RECORD remained available"
    );

    context
        .pip_uninstall()
        .arg("interrupted-cleanup")
        .assert()
        .success();
    assert!(!package.exists());
    assert!(!dist_info.exists());

    Ok(())
}

#[test]
#[cfg(unix)]
fn interrupted_uninstall_by_path_can_be_retried() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let source = context.temp_dir.join("interrupted-url");
    fs_err::create_dir_all(&source)?;
    fs_err::write(
        source.join("pyproject.toml"),
        "[project]\nname = 'interrupted-url'\nversion = '1.0.0'\n",
    )?;
    let source_url = Url::from_file_path(&source).expect("source path is a valid file URL");

    let site_packages = context.site_packages();
    let package = site_packages.join("interrupted_url");
    let dist_info = site_packages.join("interrupted_url-1.0.0.dist-info");
    fs_err::create_dir_all(&package)?;
    fs_err::create_dir_all(&dist_info)?;
    fs_err::write(
        dist_info.join("METADATA"),
        "Metadata-Version: 2.1\nName: interrupted-url\nVersion: 1.0.0\n",
    )?;
    fs_err::write(
        dist_info.join("direct_url.json"),
        format!(r#"{{"url":"{source_url}","dir_info":{{}}}}"#),
    )?;

    let record_path = dist_info.join("RECORD");
    let mut record = String::from(
        "interrupted_url-1.0.0.dist-info/METADATA,,\n\
         interrupted_url-1.0.0.dist-info/RECORD,,\n\
         interrupted_url-1.0.0.dist-info/direct_url.json,,\n",
    );
    for index in 0..1_000 {
        let relative_path = format!("interrupted_url/module_{index:04}.py");
        fs_err::write(site_packages.join(&relative_path), "")?;
        writeln!(record, "{relative_path},,")?;
    }
    fs_err::write(&record_path, record)?;

    let first_payload_file = package.join("module_0000.py");
    let last_payload_file = package.join("module_0999.py");
    let mut command = context.pip_uninstall();
    command.arg(&source);
    let interrupted = interrupt_when(&mut command, || {
        first_payload_file.exists() != last_payload_file.exists()
    })?;
    assert!(interrupted, "failed to interrupt uv during path uninstall");

    context.pip_uninstall().arg(&source).assert().success();
    assert!(!package.exists());
    assert!(!dist_info.exists());

    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn uninstall_editable_by_name() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(&format!(
        "-e {}",
        context
            .workspace_root
            .join("test/packages/flit_editable")
            .as_os_str()
            .to_str()
            .expect("Path is valid unicode")
    ))?;
    context
        .pip_sync()
        .arg(requirements_txt.path())
        .assert()
        .success();

    context.assert_command("import flit_editable").success();

    // Uninstall the editable by name.
    uv_snapshot!(context.filters(), context.pip_uninstall()
        .arg("flit-editable"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 1 package in [TIME]
     - flit-editable==0.1.0 (from file://[WORKSPACE]/test/packages/flit_editable)
    "
    );

    context.assert_command("import flit_editable").failure();

    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn uninstall_by_path() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(
        context
            .workspace_root
            .join("test/packages/flit_editable")
            .as_os_str()
            .to_str()
            .expect("Path is valid unicode"),
    )?;

    context
        .pip_sync()
        .arg(requirements_txt.path())
        .assert()
        .success();

    context.assert_command("import flit_editable").success();

    // Uninstall the editable by path.
    uv_snapshot!(context.filters(), context.pip_uninstall()
        .arg(context.workspace_root.join("test/packages/flit_editable")), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 1 package in [TIME]
     - flit-editable==0.1.0 (from file://[WORKSPACE]/test/packages/flit_editable)
    "
    );

    context.assert_command("import flit_editable").failure();

    Ok(())
}

#[test]
#[cfg(feature = "test-pypi")]
fn uninstall_duplicate_by_path() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let requirements_txt = context.temp_dir.child("requirements.txt");
    requirements_txt.write_str(
        context
            .workspace_root
            .join("test/packages/flit_editable")
            .as_os_str()
            .to_str()
            .expect("Path is valid unicode"),
    )?;

    context
        .pip_sync()
        .arg(requirements_txt.path())
        .assert()
        .success();

    context.assert_command("import flit_editable").success();

    // Uninstall the editable by both path and name.
    uv_snapshot!(context.filters(), context.pip_uninstall()
        .arg("flit-editable")
        .arg(context.workspace_root.join("test/packages/flit_editable")), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 1 package in [TIME]
     - flit-editable==0.1.0 (from file://[WORKSPACE]/test/packages/flit_editable)
    "
    );

    context.assert_command("import flit_editable").failure();

    Ok(())
}

/// Uninstall a duplicate package in a virtual environment.
#[test]
#[cfg(feature = "test-pypi")]
fn uninstall_duplicate() -> Result<()> {
    use uv_fs::copy_dir_all;

    // Sync a version of `pip` into a virtual environment.
    let context1 = uv_test::test_context!("3.12");
    let requirements_txt = context1.temp_dir.child("requirements.txt");
    requirements_txt.write_str("pip==21.3.1")?;

    // Run `pip sync`.
    context1
        .pip_sync()
        .arg(requirements_txt.path())
        .assert()
        .success();

    // Sync a different version of `pip` into a virtual environment.
    let context2 = uv_test::test_context!("3.12");
    let requirements_txt = context2.temp_dir.child("requirements.txt");
    requirements_txt.write_str("pip==22.1.1")?;

    // Run `pip sync`.
    context2
        .pip_sync()
        .arg(requirements_txt.path())
        .assert()
        .success();

    // Copy the virtual environment to a new location.
    copy_dir_all(
        context2.site_packages().join("pip-22.1.1.dist-info"),
        context1.site_packages().join("pip-22.1.1.dist-info"),
    )?;

    // Run `pip uninstall`.
    uv_snapshot!(context1.pip_uninstall()
        .arg("pip"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 2 packages in [TIME]
     - pip==21.3.1
     - pip==22.1.1
    "
    );

    Ok(())
}

/// Uninstall a `.egg-info` package in a virtual environment.
#[test]
fn uninstall_egg_info() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let site_packages = ChildPath::new(context.site_packages());

    // Manually create a `.egg-info` directory.
    site_packages
        .child("zstandard-0.22.0-py3.12.egg-info")
        .create_dir_all()?;
    site_packages
        .child("zstandard-0.22.0-py3.12.egg-info")
        .child("top_level.txt")
        .write_str("zstd")?;
    site_packages
        .child("zstandard-0.22.0-py3.12.egg-info")
        .child("SOURCES.txt")
        .write_str("")?;
    site_packages
        .child("zstandard-0.22.0-py3.12.egg-info")
        .child("PKG-INFO")
        .write_str("")?;
    site_packages
        .child("zstandard-0.22.0-py3.12.egg-info")
        .child("dependency_links.txt")
        .write_str("")?;
    site_packages
        .child("zstandard-0.22.0-py3.12.egg-info")
        .child("entry_points.txt")
        .write_str("")?;

    // Manually create the package directory.
    site_packages.child("zstd").create_dir_all()?;
    site_packages
        .child("zstd")
        .child("__init__.py")
        .write_str("")?;

    // Run `pip uninstall`.
    uv_snapshot!(context.pip_uninstall()
        .arg("zstandard"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 1 package in [TIME]
     - zstandard==0.22.0
    ");

    Ok(())
}

fn normcase(s: &str) -> String {
    if cfg!(windows) {
        s.replace('/', "\\").to_lowercase()
    } else {
        s.to_owned()
    }
}

/// Uninstall a legacy editable package in a virtual environment.
#[test]
fn uninstall_legacy_editable() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let site_packages = ChildPath::new(context.site_packages());

    let target = context.temp_dir.child("zstandard_project");
    target.child("zstd").create_dir_all()?;
    target.child("zstd").child("__init__.py").write_str("")?;

    target.child("zstandard.egg-info").create_dir_all()?;
    target
        .child("zstandard.egg-info")
        .child("PKG-INFO")
        .write_str(
            "Metadata-Version: 2.1
Name: zstandard
Version: 0.22.0
",
        )?;

    site_packages
        .child("zstandard.egg-link")
        .write_str(target.path().to_str().unwrap())?;

    site_packages.child("easy-install.pth").write_str(&format!(
        "something\n{}\nanother thing\n",
        normcase(target.path().to_str().unwrap())
    ))?;

    // Run `pip uninstall`.
    uv_snapshot!(context.pip_uninstall()
        .arg("zstandard"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Uninstalled 1 package in [TIME]
     - zstandard==0.22.0
    ");

    // The entry in `easy-install.pth` should be removed.
    assert_eq!(
        fs_err::read_to_string(site_packages.child("easy-install.pth"))?,
        "something\nanother thing\n",
        "easy-install.pth should not contain the path to the uninstalled package"
    );
    // The `.egg-link` file should be removed.
    assert!(!site_packages.child("zstandard.egg-link").exists());
    // The `.egg-info` directory should still exist.
    assert!(target.child("zstandard.egg-info").exists());

    Ok(())
}

#[test]
fn dry_run_uninstall_egg_info() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let site_packages = ChildPath::new(context.site_packages());

    // Manually create a `.egg-info` directory.
    site_packages
        .child("zstandard-0.22.0-py3.12.egg-info")
        .create_dir_all()?;
    site_packages
        .child("zstandard-0.22.0-py3.12.egg-info")
        .child("top_level.txt")
        .write_str("zstd")?;
    site_packages
        .child("zstandard-0.22.0-py3.12.egg-info")
        .child("SOURCES.txt")
        .write_str("")?;
    site_packages
        .child("zstandard-0.22.0-py3.12.egg-info")
        .child("PKG-INFO")
        .write_str("")?;
    site_packages
        .child("zstandard-0.22.0-py3.12.egg-info")
        .child("dependency_links.txt")
        .write_str("")?;
    site_packages
        .child("zstandard-0.22.0-py3.12.egg-info")
        .child("entry_points.txt")
        .write_str("")?;

    // Manually create the package directory.
    site_packages.child("zstd").create_dir_all()?;
    site_packages
        .child("zstd")
        .child("__init__.py")
        .write_str("")?;

    // Run `pip uninstall`.
    uv_snapshot!(context.pip_uninstall()
        .arg("--dry-run")
        .arg("zstandard"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Would uninstall 1 package
     - zstandard==0.22.0
    ");

    // The `.egg-info` directory should still exist.
    assert!(
        site_packages
            .child("zstandard-0.22.0-py3.12.egg-info")
            .exists()
    );
    // The package directory should still exist.
    assert!(site_packages.child("zstd").child("__init__.py").exists());

    Ok(())
}

/// Uninstall must not remove files outside the install scheme.
///
/// A malformed or malicious wheel can include path-traversal entries
/// (e.g. `../../../../../etc/passwd`) in its RECORD file. During uninstall those entries are joined
/// with the site-packages directory and could cause deletion of files outside the installation
/// scheme.
#[test]
fn uninstall_record_path_traversal() -> Result<()> {
    // The traversal-depth count differs between Unix (`.venv/lib/pythonX.Y/site-packages`)
    // and Windows (`.venv/Lib/site-packages`), so normalize the `../` sequence in the warning.
    let context = uv_test::test_context!("3.12").with_filter((
        r"(\.\./)+traversal_target\.txt",
        "[..]/traversal_target.txt",
    ));

    context
        .init()
        .arg("--lib")
        .arg("evilpkg")
        .assert()
        .success();
    context.pip_install().arg("./evilpkg").assert().success();

    // Build the relative traversal path from site-packages to a target file outside
    // site-packages but inside the test temp dir. RECORD uses forward slashes, even on
    // Windows, and the environment layout (and thus the traversal depth) differs by platform,
    // so we construct the path manually and filter the leading `../` sequence out of the
    // snapshot above.
    let target_file = context.temp_dir.child("traversal_target.txt");
    target_file.write_str("I should not be deleted")?;
    // Canonicalize the temp dir, since `site_packages` is built from a canonicalized path
    // (with `\\?\`), which would otherwise make `strip_prefix` fail.
    let canonical_temp_dir = context.temp_dir.canonicalize()?;
    let depth = context
        .site_packages()
        .strip_prefix(&canonical_temp_dir)?
        .components()
        .count();
    let traversal_record = format!("{}traversal_target.txt", "../".repeat(depth));

    let record_file = context
        .site_packages()
        .join("evilpkg-0.1.0.dist-info/RECORD");
    let record = fs_err::read_to_string(&record_file)?;
    let record = format!("{}\n{},,0\n", record.trim(), traversal_record);
    fs_err::write(record_file, &record)?;

    let init_py = context.site_packages().join("evilpkg/__init__.py");
    assert!(context.site_packages().join(&traversal_record).exists());
    assert!(init_py.exists());

    uv_snapshot!(context.filters(), context.pip_uninstall()
        .arg("evilpkg"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Invalid RECORD entry in evilpkg==0.1.0 (from file://[TEMP_DIR]/evilpkg) that escapes the Python environment, skipping: [..]/traversal_target.txt
    Uninstalled 1 package in [TIME]
     - evilpkg==0.1.0 (from file://[TEMP_DIR]/evilpkg)
    ");

    // The regular package files have been removed, while the file outside the scheme still exists.
    assert!(target_file.exists());
    assert!(!init_py.exists());

    Ok(())
}

/// Egg `top_level.txt` entries must be top-level names, not paths.
#[test]
fn uninstall_egg_info_top_level_path_traversal() -> Result<()> {
    // The traversal-depth count differs between Unix (`.venv/lib/pythonX.Y/site-packages`)
    // and Windows (`.venv/Lib/site-packages`), so normalize the `../` sequence in the warning.
    let context = uv_test::test_context!("3.12")
        .with_filter((r"(\.\./)+traversal_target", "[..]/traversal_target"));

    let site_packages = ChildPath::new(context.site_packages());

    // Manually create a `name-version.egg-info` directory, which is recognized by the shared egg
    // filename parser.
    let egg_info = site_packages.child("evilpkg-0.1.0.egg-info");
    egg_info.create_dir_all()?;

    // The traversal target is outside site-packages but inside the environment, so a wheel RECORD entry
    // could validly target this scheme area. An egg `top_level.txt` entry must not.
    let target_dir = context.venv.child("traversal_target");
    let target_file = target_dir.child("secret.txt");
    target_file.write_str("I should not be deleted")?;

    let depth = context
        .site_packages()
        .strip_prefix(context.venv.path())?
        .components()
        .count();
    let traversal_entry = format!("{}traversal_target", "../".repeat(depth));
    assert!(context.site_packages().join(&traversal_entry).exists());

    egg_info
        .child("top_level.txt")
        .write_str(&format!("evilpkg\n{traversal_entry}\n"))?;

    let init_py = site_packages.child("evilpkg").child("__init__.py");
    init_py.touch()?;

    uv_snapshot!(context.filters(), context.pip_uninstall()
        .arg("evilpkg"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Invalid `top_level.txt` entry in evilpkg==0.1.0 that is not a top-level module or package, skipping: [..]/traversal_target
    Uninstalled 1 package in [TIME]
     - evilpkg==0.1.0
    ");

    assert!(target_dir.exists());
    assert!(target_file.exists());
    assert!(!init_py.exists());
    assert!(!egg_info.exists());

    Ok(())
}

/// Windows drive-relative paths are not valid `top_level.txt` entries.
#[cfg(windows)]
#[test]
fn uninstall_egg_info_top_level_drive_relative() -> Result<()> {
    let context = uv_test::test_context!("3.12")
        .with_filter((r"[A-Za-z]:traversal_target", "[DRIVE]:traversal_target"));
    let site_packages = ChildPath::new(context.site_packages());

    let egg_info = site_packages.child("evilpkg-0.1.0.egg-info");
    egg_info.create_dir_all()?;

    let drive = match context.temp_dir.path().components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => drive,
            prefix => anyhow::bail!("expected a disk path, found {prefix:?}"),
        },
        component => anyhow::bail!("expected a Windows path prefix, found {component:?}"),
    };
    let traversal_entry = format!("{}:traversal_target", char::from(drive));

    // Commands run from `context.temp_dir`, so this drive-relative path resolves there rather than
    // below `site-packages`.
    let target_file = context
        .temp_dir
        .child("traversal_target")
        .child("secret.txt");
    target_file.write_str("I should not be deleted")?;
    egg_info
        .child("top_level.txt")
        .write_str(&format!("evilpkg\n{traversal_entry}\n"))?;

    let init_py = site_packages.child("evilpkg").child("__init__.py");
    init_py.touch()?;

    uv_snapshot!(context.filters(), context.pip_uninstall()
        .arg("evilpkg"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Invalid `top_level.txt` entry in evilpkg==0.1.0 that is not a top-level module or package, skipping: [DRIVE]:traversal_target
    Uninstalled 1 package in [TIME]
     - evilpkg==0.1.0
    ");

    assert!(target_file.exists());
    assert!(!init_py.exists());
    assert!(!egg_info.exists());

    Ok(())
}

/// `--yes` is accepted for `pip uninstall` compatibility, but emits a warning.
#[test]
fn yes_flag() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.filters(), context.pip_uninstall()
        .arg("--yes")
        .arg("flask"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `--yes` has no effect (uv never asks for confirmation)
    warning: Skipping flask as it is not installed
    warning: No packages to uninstall
    "
    );
}

/// `-y` is accepted for `pip uninstall` compatibility, but emits a warning.
#[test]
fn yes_short_flag() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.filters(), context.pip_uninstall()
        .arg("-y")
        .arg("flask"), @"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: `--yes` has no effect (uv never asks for confirmation)
    warning: Skipping flask as it is not installed
    warning: No packages to uninstall
    "
    );
}

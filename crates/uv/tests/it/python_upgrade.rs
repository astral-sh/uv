use crate::common::{TestContext, uv_snapshot};
use anyhow::Result;
use assert_fs::fixture::FileTouch;
use assert_fs::prelude::PathChild;

use uv_static::EnvVars;

#[test]
fn python_upgrade() {
    let context: TestContext = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    // Install an earlier patch version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.10.17"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.10.17 in [TIME]
     + cpython-3.10.17-[PLATFORM] (python3.10)
    ");

    // Don't accept patch version as argument to upgrade command
    uv_snapshot!(context.filters(), context.python_upgrade().arg("--preview").arg("3.10.17"), @r"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    error: `uv python upgrade` only accepts minor versions
    ");

    // Upgrade patch version
    uv_snapshot!(context.filters(), context.python_upgrade().arg("--preview").arg("3.10"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.10.18 in [TIME]
     + cpython-3.10.18-[PLATFORM] (python3.10)
    ");

    // Should be a no-op when already upgraded
    uv_snapshot!(context.filters(), context.python_upgrade().arg("--preview").arg("3.10"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");
}

#[test]
fn python_upgrade_without_version() {
    let context: TestContext = TestContext::new_with_versions(&[])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    // Should be a no-op when no versions have been installed
    uv_snapshot!(context.filters(), context.python_upgrade().arg("--preview"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    There are no installed versions to upgrade
    ");

    // Install earlier patch versions for different minor versions
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.11.8").arg("3.12.8").arg("3.13.1"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 3 versions in [TIME]
     + cpython-3.11.8-[PLATFORM] (python3.11)
     + cpython-3.12.8-[PLATFORM] (python3.12)
     + cpython-3.13.1-[PLATFORM] (python3.13)
    ");

    let mut filters = context.filters().clone();
    filters.push((r"3.13.\d+", "3.13.[X]"));

    // Upgrade one patch version
    uv_snapshot!(filters, context.python_upgrade().arg("--preview").arg("3.13"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.13.[X] in [TIME]
     + cpython-3.13.[X]-[PLATFORM] (python3.13)
    ");

    // Providing no minor version to `uv python upgrade` should upgrade the rest
    // of the patch versions
    uv_snapshot!(context.filters(), context.python_upgrade().arg("--preview"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed 2 versions in [TIME]
     + cpython-3.11.13-[PLATFORM] (python3.11)
     + cpython-3.12.11-[PLATFORM] (python3.12)
    ");

    // Should be a no-op when every version is already upgraded
    uv_snapshot!(context.filters(), context.python_upgrade().arg("--preview"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    All versions already on latest supported patch release
    ");
}

#[test]
fn python_upgrade_transparent_from_venv() {
    let context: TestContext = TestContext::new_with_versions(&["3.13"])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    // Install an earlier patch version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.10.17"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.10.17 in [TIME]
     + cpython-3.10.17-[PLATFORM] (python3.10)
    ");

    // Create a virtual environment
    uv_snapshot!(context.filters(), context.venv(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.10.17
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    ");

    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.10.17

    ----- stderr -----
    "
    );

    let second_venv = ".venv2";

    // Create a second virtual environment with minor version request
    uv_snapshot!(context.filters(), context.venv().arg(second_venv).arg("-p").arg("3.10"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.10.17
    Creating virtual environment at: .venv2
    Activate with: source .venv2/[BIN]/activate
    ");

    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version")
        .env(EnvVars::VIRTUAL_ENV, second_venv), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.10.17

    ----- stderr -----
    "
    );

    // Upgrade patch version
    uv_snapshot!(context.filters(), context.python_upgrade().arg("--preview").arg("3.10"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.10.18 in [TIME]
     + cpython-3.10.18-[PLATFORM] (python3.10)
    ");

    // First virtual environment should reflect upgraded patch
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.10.18

    ----- stderr -----
    "
    );

    // Second virtual environment should reflect upgraded patch
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version")
        .env(EnvVars::VIRTUAL_ENV, second_venv), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.10.18

    ----- stderr -----
    "
    );
}

// Installing Python in preview mode should not prevent virtual environments
// from transparently upgrading.
#[test]
fn python_upgrade_transparent_from_venv_preview() {
    let context: TestContext = TestContext::new_with_versions(&["3.13"])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    // Install an earlier patch version using `--preview`
    uv_snapshot!(context.filters(), context.python_install().arg("3.10.17").arg("--preview"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.10.17 in [TIME]
     + cpython-3.10.17-[PLATFORM] (python3.10)
    ");

    // Create a virtual environment
    uv_snapshot!(context.filters(), context.venv().arg("-p").arg("3.10"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.10.17
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    ");

    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.10.17

    ----- stderr -----
    "
    );

    // Upgrade patch version
    uv_snapshot!(context.filters(), context.python_upgrade().arg("--preview").arg("3.10"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.10.18 in [TIME]
     + cpython-3.10.18-[PLATFORM] (python3.10)
    ");

    // Virtual environment should reflect upgraded patch
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.10.18

    ----- stderr -----
    "
    );
}

#[test]
fn python_upgrade_ignored_with_python_pin() {
    let context: TestContext = TestContext::new_with_versions(&["3.13"])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    // Install an earlier patch version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.10.17"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.10.17 in [TIME]
     + cpython-3.10.17-[PLATFORM] (python3.10)
    ");

    // Create a virtual environment
    uv_snapshot!(context.filters(), context.venv(), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.10.17
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    ");

    // Pin to older patch version
    uv_snapshot!(context.filters(), context.python_pin().arg("3.10.17"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Pinned `.python-version` to `3.10.17`

    ----- stderr -----
    ");

    // Upgrade patch version
    uv_snapshot!(context.filters(), context.python_upgrade().arg("--preview").arg("3.10"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.10.18 in [TIME]
     + cpython-3.10.18-[PLATFORM] (python3.10)
    ");

    // Virtual environment should continue to respect pinned patch version
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.10.17

    ----- stderr -----
    "
    );
}

// Virtual environments only record minor versions. `uv venv -p 3.x.y` will
// not prevent transparent upgrades.
#[test]
fn python_no_transparent_upgrade_with_venv_patch_specification() {
    let context: TestContext = TestContext::new_with_versions(&["3.13"])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    // Install an earlier patch version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.10.17"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.10.17 in [TIME]
     + cpython-3.10.17-[PLATFORM] (python3.10)
    ");

    // Create a virtual environment with a patch version
    uv_snapshot!(context.filters(), context.venv().arg("-p").arg("3.10.17"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.10.17
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    ");

    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.10.17

    ----- stderr -----
    "
    );

    // Upgrade patch version
    uv_snapshot!(context.filters(), context.python_upgrade().arg("--preview").arg("3.10"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.10.18 in [TIME]
     + cpython-3.10.18-[PLATFORM] (python3.10)
    ");

    // The virtual environment Python version is transparently upgraded.
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.10.17

    ----- stderr -----
    "
    );
}

// Transparent upgrades should work for virtual environments created within
// virtual environments.
#[test]
fn python_transparent_upgrade_venv_venv() {
    let context: TestContext = TestContext::new_with_versions(&["3.13"])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_filtered_virtualenv_bin()
        .with_managed_python_dirs();

    // Install an earlier patch version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.10.17"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.10.17 in [TIME]
     + cpython-3.10.17-[PLATFORM] (python3.10)
    ");

    // Create an initial virtual environment
    uv_snapshot!(context.filters(), context.venv().arg("-p").arg("3.10"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.10.17
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    ");

    let venv_python = if cfg!(windows) {
        context.venv.child("Scripts/python.exe")
    } else {
        context.venv.child("bin/python")
    };

    let second_venv = ".venv2";

    // Create a new virtual environment from within a virtual environment
    uv_snapshot!(context.filters(), context.venv()
        .arg(second_venv)
        .arg("-p").arg(venv_python.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.10.17 interpreter at: .venv/[BIN]/python
    Creating virtual environment at: .venv2
    Activate with: source .venv2/[BIN]/activate
    ");

    // Check version from within second virtual environment
    uv_snapshot!(context.filters(), context.run()
        .arg("python").arg("--version")
        .env(EnvVars::VIRTUAL_ENV, second_venv), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.10.17

    ----- stderr -----
    "
    );

    // Upgrade patch version
    uv_snapshot!(context.filters(), context.python_upgrade().arg("--preview").arg("3.10"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.10.18 in [TIME]
     + cpython-3.10.18-[PLATFORM] (python3.10)
    ");

    // Should have transparently upgraded in second virtual environment
    uv_snapshot!(context.filters(), context.run()
        .arg("python").arg("--version")
        .env(EnvVars::VIRTUAL_ENV, second_venv), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.10.18

    ----- stderr -----
    "
    );
}

// Transparent upgrades should work for virtual environments created using
// the `venv` module.
#[test]
fn python_upgrade_transparent_from_venv_module() {
    let context = TestContext::new_with_versions(&["3.13"])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_python_install_bin();

    let bin_dir = context.temp_dir.child("bin");

    // Install earlier patch version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.12.9"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.9 in [TIME]
     + cpython-3.12.9-[PLATFORM] (python3.12)
    ");

    // Create a virtual environment using venv module
    uv_snapshot!(context.filters(), context.run().arg("python").arg("-m").arg("venv").arg(context.venv.as_os_str()).arg("--without-pip")
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.9

    ----- stderr -----
    "
    );

    // Upgrade patch version
    uv_snapshot!(context.filters(), context.python_upgrade().arg("--preview").arg("3.12"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.11 in [TIME]
     + cpython-3.12.11-[PLATFORM] (python3.12)
    "
    );

    // Virtual environment should reflect upgraded patch
    uv_snapshot!(context.filters(), context.run().arg("python").arg("--version"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.12.11

    ----- stderr -----
    "
    );
}

// Transparent Python upgrades should work in environments created using
// the `venv` module within an existing virtual environment.
#[test]
fn python_upgrade_transparent_from_venv_module_in_venv() {
    let context = TestContext::new_with_versions(&["3.13"])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs()
        .with_filtered_python_install_bin();

    let bin_dir = context.temp_dir.child("bin");

    // Install earlier patch version
    uv_snapshot!(context.filters(), context.python_install().arg("--preview").arg("3.10.17"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.10.17 in [TIME]
     + cpython-3.10.17-[PLATFORM] (python3.10)
    ");

    // Create first virtual environment
    uv_snapshot!(context.filters(), context.venv().arg("-p").arg("3.10"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Using CPython 3.10.17
    Creating virtual environment at: .venv
    Activate with: source .venv/[BIN]/activate
    ");

    let second_venv = ".venv2";

    // Create a virtual environment using `venv`` module from within the first virtual environment.
    uv_snapshot!(context.filters(), context.run()
        .arg("python").arg("-m").arg("venv").arg(second_venv).arg("--without-pip")
        .env(EnvVars::PATH, bin_dir.as_os_str()), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    // Check version within second virtual environment
    uv_snapshot!(context.filters(), context.run()
        .env(EnvVars::VIRTUAL_ENV, second_venv)
        .arg("python").arg("--version"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.10.17

    ----- stderr -----
    "
    );

    // Upgrade patch version
    uv_snapshot!(context.filters(), context.python_upgrade().arg("--preview").arg("3.10"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.10.18 in [TIME]
     + cpython-3.10.18-[PLATFORM] (python3.10)
    "
    );

    // Second virtual environment should reflect upgraded patch.
    uv_snapshot!(context.filters(), context.run()
        .env(EnvVars::VIRTUAL_ENV, second_venv)
        .arg("python").arg("--version"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Python 3.10.18

    ----- stderr -----
    "
    );
}

// Tests that `uv python upgrade 3.12` will warn if trying to install over non-managed
// interpreter.
#[test]
fn python_upgrade_force_install() -> Result<()> {
    let context = TestContext::new_with_versions(&["3.13"])
        .with_filtered_python_keys()
        .with_filtered_exe_suffix()
        .with_managed_python_dirs();

    context
        .bin_dir
        .child(format!("python3.12{}", std::env::consts::EXE_SUFFIX))
        .touch()?;

    // Try to upgrade with a non-managed interpreter installed in `bin`.
    uv_snapshot!(context.filters(), context.python_upgrade().arg("--preview").arg("3.12"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    warning: Executable already exists at `[BIN]/python3.12` but is not managed by uv; use `uv python install 3.12 --force` to replace it
    Installed Python 3.12.11 in [TIME]
     + cpython-3.12.11-[PLATFORM]
    ");

    // Force the `bin` install.
    uv_snapshot!(context.filters(), context.python_install().arg("3.12").arg("--force").arg("--preview").arg("3.12"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Installed Python 3.12.11 in [TIME]
     + cpython-3.12.11-[PLATFORM] (python3.12)
    ");

    Ok(())
}

use std::collections::BTreeMap;
use std::fmt::Write;

use anyhow::Result;
use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use indoc::{formatdoc, indoc};

use uv_test::packse::generate_wheel;
use uv_test::{TestContext, capture_uv_snapshot, uv_snapshot};

fn wheel(context: &TestContext, version: &str, tag: &str) -> Result<()> {
    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    let (filename, bytes) = generate_wheel(
        &"demo".parse()?,
        &version.parse()?,
        &[],
        &BTreeMap::new(),
        None,
        tag,
        &[],
    );
    links.child(filename).write_binary(&bytes)?;
    Ok(())
}

/// Resolution must cover both ABIs at the required libc baseline, on the same architecture.
#[test]
fn required_environments_both_abis() -> Result<()> {
    let context = uv_test::test_context!("3.13");
    for (version, tag) in [
        ("1.0.0", "cp313-cp313-manylinux_2_17_x86_64"),
        ("1.0.0", "cp313-cp313t-manylinux_2_17_x86_64"),
        // The regular wheel is too new for the glibc baseline.
        ("2.0.0", "cp313-cp313-manylinux_2_34_x86_64"),
        ("2.0.0", "cp313-cp313t-manylinux_2_17_x86_64"),
        // The free-threaded wheel has the wrong architecture.
        ("3.0.0", "cp313-cp313-manylinux_2_17_x86_64"),
        ("3.0.0", "cp313-cp313t-manylinux_2_17_aarch64"),
        // Only one of the two required ABIs is available.
        ("4.0.0", "cp313-cp313t-manylinux_2_17_x86_64"),
        ("5.0.0", "cp313-cp313-manylinux_2_17_x86_64"),
    ] {
        wheel(&context, version, tag)?;
    }
    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.13,<3.14"
        dependencies = ["demo"]

        [tool.uv]
        preview-features = ["sys-abi-features", "minimum-libc-version"]
        no-build = true
        no-index = true
        find-links = ["links"]
        minimum-libc-version = { glibc = "2.28" }
        required-environments = [
            "sys_platform == 'linux' and platform_machine == 'x86_64' and 'gil-enabled' in sys_abi_features",
            "sys_platform == 'linux' and platform_machine == 'x86_64' and 'free-threading' in sys_abi_features",
        ]
    "#})?;
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");
    uv_snapshot!(context.filters(), context.export().args(["--frozen", "--no-hashes", "--no-header", "--no-annotate"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    demo==4.0.0 ; 'free-threading' in sys_abi_features
    demo==5.0.0 ; 'free-threading' not in sys_abi_features
    ");
    let original = context.read("pyproject.toml");
    // A pinned release must provide both ABIs, with each wheel satisfying the complete target.
    let mut results = String::new();
    for version in ["1", "2", "3", "4", "5"] {
        pyproject.write_str(&original.replace(
            "dependencies = [\"demo\"]",
            &format!("dependencies = [\"demo=={version}\"]"),
        ))?;
        let output = capture_uv_snapshot!(context.filters(), context.lock().arg("--offline"));
        writeln!(results, "demo=={version}\n{output}")?;
    }
    insta::assert_snapshot!(results, @"
    demo==1
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Updated demo v4.0.0, v5.0.0 -> v1.0.0

    demo==2
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies for split (markers: python_full_version == '3.13.*' and 'free-threading' not in sys_abi_features)
      cause: Because demo==2.0.0 has no `platform_machine == 'x86_64' and sys_platform == 'linux' and 'gil-enabled' in sys_abi_features`-compatible wheels and your project depends on demo==2, we can conclude that your project's requirements are unsatisfiable.

    demo==3
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies for split (markers: python_full_version == '3.13.*' and platform_machine == 'x86_64' and 'free-threading' in sys_abi_features)
      cause: Because demo==3.0.0 has no `platform_machine == 'x86_64' and sys_platform == 'linux' and 'free-threading' in sys_abi_features`-compatible wheels and your project depends on demo==3, we can conclude that your project's requirements are unsatisfiable.

    hint: The resolution failed for an environment that is not the current one, consider limiting the environments with `tool.uv.environments`.

    demo==4
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies for split (markers: python_full_version == '3.13.*' and 'free-threading' not in sys_abi_features)
      cause: Because demo==4.0.0 has no `platform_machine == 'x86_64' and sys_platform == 'linux' and 'gil-enabled' in sys_abi_features`-compatible wheels and your project depends on demo==4, we can conclude that your project's requirements are unsatisfiable.

    demo==5
    exit_code: 1 (failure)
    ----- stderr -----
    error: No solution found when resolving dependencies for split (markers: python_full_version == '3.13.*' and 'free-threading' in sys_abi_features)
      cause: Because demo==5.0.0 has no `platform_machine == 'x86_64' and sys_platform == 'linux' and 'free-threading' in sys_abi_features`-compatible wheels and your project depends on demo==5, we can conclude that your project's requirements are unsatisfiable.

    hint: The resolution failed for an environment that is not the current one, consider limiting the environments with `tool.uv.environments`.
    ");
    // One ABI-independent wheel can satisfy both required environments.
    wheel(&context, "6.0.0", "py3-none-any")?;
    pyproject.write_str(
        &original.replace("dependencies = [\"demo\"]", "dependencies = [\"demo==6\"]"),
    )?;
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Updated demo v1.0.0 -> v6.0.0
    ");
    Ok(())
}

/// Each ABI can be required separately, including by excluding free-threading.
#[test]
fn required_environments_single_abi() -> Result<()> {
    let context = uv_test::test_context!("3.13");
    wheel(&context, "1.0.0", "cp313-cp313-manylinux_2_17_x86_64")?;
    wheel(&context, "2.0.0", "cp313-cp313t-manylinux_2_17_x86_64")?;
    let pyproject = context.temp_dir.child("pyproject.toml");
    for (marker, version) in [
        ("'gil-enabled' in sys_abi_features", "1.0.0"),
        (
            "'free-threading' not in sys_abi_features and platform_python_implementation == 'CPython'",
            "1.0.0",
        ),
        ("'free-threading' in sys_abi_features", "2.0.0"),
    ] {
        pyproject.write_str(&formatdoc! {r#"
            [project]
            name = "project"
            version = "0.1.0"
            requires-python = ">=3.13,<3.14"
            dependencies = ["demo"]

            [tool.uv]
            preview-features = ["sys-abi-features"]
            no-index = true
            find-links = ["links"]
            required-environments = ["sys_platform == 'linux' and platform_machine == 'x86_64' and {marker}"]
        "#})?;
        context
            .lock()
            .args(["--offline", "--upgrade"])
            .assert()
            .success();
        let lock: toml::Value = toml::from_str(&context.read("uv.lock"))?;
        assert_eq!(lock["package"][0]["name"].as_str(), Some("demo"));
        assert_eq!(
            lock["package"][0]["version"].as_str(),
            Some(version),
            "{marker}"
        );
    }
    Ok(())
}

#[test]
fn sys_abi_features_preview_gate() -> Result<()> {
    let context = uv_test::test_context!("3.13");
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.13"
        dependencies = []

        [tool.uv]
        required-environments = ["'gil-enabled' in sys_abi_features"]
    "#})?;
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The `sys-abi-features` feature is experimental and requires `--preview-features sys-abi-features`
    ");
    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--preview-features", "sys-abi-features"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    Ok(())
}

/// One lock can select different dependencies for regular and free-threaded interpreters.
#[test]
fn sys_abi_features_dependency_forks() -> Result<()> {
    let context = uv_test::test_context!("3.13")
        .with_managed_python_dirs()
        .with_filtered_python_keys()
        .with_filtered_latest_python_versions()
        .with_filtered_python_install_bin()
        .with_filtered_python_names()
        .with_filtered_exe_suffix();
    context.python_install().arg("3.13t").assert().success();
    wheel(&context, "1.0.0", "py3-none-any")?;
    wheel(&context, "2.0.0", "py3-none-any")?;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.13"
        dependencies = [
            "demo==1; 'gil-enabled' in sys_abi_features",
            "demo==2; 'free-threading' in sys_abi_features",
        ]

        [tool.uv]
        preview-features = ["sys-abi-features"]
        no-index = true
        find-links = ["links"]
    "#})?;
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");
    uv_snapshot!(context.filters(), context.export().args(["--frozen", "--no-hashes", "--no-header", "--no-annotate"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    demo==1.0.0 ; 'gil-enabled' in sys_abi_features
    demo==2.0.0 ; 'free-threading' in sys_abi_features and 'gil-enabled' not in sys_abi_features
    ");
    uv_snapshot!(context.filters(), context.sync().args(["--locked", "--offline", "--python", "3.13"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 3 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + demo==1.0.0
    ");
    uv_snapshot!(context.filters(), context.sync().args(["--locked", "--offline", "--python", "3.13t"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Using CPython 3.13.[X]+freethreaded
    Removed virtual environment at: .venv
    Creating virtual environment at: .venv
    Resolved 3 packages in [TIME]
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + demo==2.0.0
    ");
    Ok(())
}

/// Cross-platform resolution must use the target's bitness rather than the host's.
#[test]
fn sys_abi_features_target_bitness() -> Result<()> {
    let context = uv_test::test_context!("3.13");
    wheel(&context, "1.0.0", "py3-none-any")?;
    wheel(&context, "2.0.0", "py3-none-any")?;
    context
        .temp_dir
        .child("requirements.in")
        .write_str(indoc! {r"
        demo==1; '32-bit' in sys_abi_features
        demo==2; '64-bit' in sys_abi_features
    "})?;
    uv_snapshot!(context.filters(), context.pip_compile()
        .args(["requirements.in", "--offline", "--no-index", "--find-links", "links", "--preview-features", "sys-abi-features", "--python-platform", "i686-pc-windows-msvc", "--emit-marker-expression", "--no-header", "--no-annotate"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    # Pinned dependencies known to be valid for:
    #    '32-bit' in sys_abi_features and '64-bit' not in sys_abi_features
    demo==1.0.0

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    uv_snapshot!(context.filters(), context.pip_compile()
        .args(["requirements.in", "--offline", "--no-index", "--find-links", "links", "--preview-features", "sys-abi-features", "--python-platform", "x86_64-pc-windows-msvc", "--emit-marker-expression", "--no-header", "--no-annotate"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    # Pinned dependencies known to be valid for:
    #    '32-bit' not in sys_abi_features and '64-bit' in sys_abi_features
    demo==2.0.0

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    Ok(())
}

#[test]
fn required_environments_bitness() -> Result<()> {
    let context = uv_test::test_context!("3.13");
    wheel(&context, "1.0.0", "cp313-cp313-win32")?;
    wheel(&context, "2.0.0", "cp313-cp313-win_amd64")?;
    for (bits, version) in [(32, "1.0.0"), (64, "2.0.0")] {
        context
            .temp_dir
            .child("pyproject.toml")
            .write_str(&formatdoc! {r#"
            [project]
            name = "project"
            version = "0.1.0"
            requires-python = ">=3.13,<3.14"
            dependencies = ["demo"]

            [tool.uv]
            preview-features = ["sys-abi-features"]
            no-index = true
            find-links = ["links"]
            required-environments = ["sys_platform == 'win32' and '{bits}-bit' in sys_abi_features"]
        "#})?;
        context
            .lock()
            .args(["--offline", "--upgrade"])
            .assert()
            .success();
        let lock: toml::Value = toml::from_str(&context.read("uv.lock"))?;
        assert_eq!(lock["package"][0]["name"].as_str(), Some("demo"));
        assert_eq!(lock["package"][0]["version"].as_str(), Some(version));
    }
    Ok(())
}

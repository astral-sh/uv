use std::collections::BTreeMap;

use anyhow::{Context, Result};
use assert_fs::fixture::ChildPath;
use assert_fs::prelude::*;
use indoc::{formatdoc, indoc};
use insta::assert_snapshot;
use sha2::{Digest, Sha256};

use uv_test::archive::write_tar_gz;
use uv_test::find_links::FindLinksServer;
use uv_test::packse::generate_wheel;
use uv_test::{TestContext, uv_snapshot};

fn wheel(context: &TestContext, name: &str, version: &str, tag: &str) -> Result<ChildPath> {
    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    let (filename, bytes) = generate_wheel(
        &name.parse()?,
        &version.parse()?,
        &[],
        &BTreeMap::new(),
        None,
        tag,
    );
    let wheel = links.child(filename);
    wheel.write_binary(&bytes)?;
    Ok(wheel)
}

/// The glibc floor filters Linux artifacts without removing wheels for other platforms.
#[test]
fn minimum_libc_filters_locked_wheels() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    for tag in [
        "cp312-cp312-manylinux_2_17_x86_64",
        "cp312-cp312-manylinux_2_34_x86_64",
        "cp312-cp312-musllinux_1_2_x86_64",
        "cp312-cp312-macosx_11_0_arm64",
        "cp312-cp312-win_amd64",
    ] {
        wheel(&context, "demo", "1.0.0", tag)?;
    }

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["demo"]

        [tool.uv]
        no-index = true
        find-links = ["links"]
        required-environments = ["sys_platform == 'linux' and platform_machine == 'x86_64'"]
    "#})?;
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @r"
        exit_code: 0 (success)
        ----- stderr -----
        Resolved 2 packages in [TIME]
    ");
    let lock = context.read("uv.lock");
    insta::with_settings!({filters => context.filters()}, {
        assert_snapshot!(lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        required-markers = [
            "platform_machine == 'x86_64' and sys_platform == 'linux'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "demo"
        version = "1.0.0"
        source = { registry = "links" }
        wheels = [
            { path = "demo-1.0.0-cp312-cp312-manylinux_2_17_x86_64.whl" },
            { path = "demo-1.0.0-cp312-cp312-manylinux_2_34_x86_64.whl" },
            { path = "demo-1.0.0-cp312-cp312-musllinux_1_2_x86_64.whl" },
            { path = "demo-1.0.0-cp312-cp312-macosx_11_0_arm64.whl" },
            { path = "demo-1.0.0-cp312-cp312-win_amd64.whl" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "demo" },
        ]

        [package.metadata]
        requires-dist = [{ name = "demo" }]
        "#);
    });

    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["demo"]

        [tool.uv]
        no-index = true
        find-links = ["links"]
        required-environments = ["sys_platform == 'linux' and platform_machine == 'x86_64'"]
        minimum-libc-version = { glibc = "2.31" }
    "#})?;
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Setting `minimum-libc-version` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    Resolved 2 packages in [TIME]
    ");
    let lock = context.read("uv.lock");
    insta::with_settings!({filters => context.filters()}, {
        assert_snapshot!(lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        required-markers = [
            "platform_machine == 'x86_64' and sys_platform == 'linux'",
        ]

        [options]
        minimum-libc-version = { glibc = "2.31" }
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "demo"
        version = "1.0.0"
        source = { registry = "links" }
        wheels = [
            { path = "demo-1.0.0-cp312-cp312-manylinux_2_17_x86_64.whl" },
            { path = "demo-1.0.0-cp312-cp312-macosx_11_0_arm64.whl" },
            { path = "demo-1.0.0-cp312-cp312-win_amd64.whl" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "demo" },
        ]

        [package.metadata]
        requires-dist = [{ name = "demo" }]
        "#);
    });

    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked", "--preview-features", "minimum-libc-version"]), @r"
        exit_code: 0 (success)
        ----- stderr -----
        Resolved 2 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context.pip_compile().args(["pyproject.toml", "--universal", "--format", "pylock.toml", "--offline", "--no-header", "--preview-features", "minimum-libc-version"]), @r#"
    exit_code: 0 (success)
    ----- stdout -----
    lock-version = "1.0"
    created-by = "uv"
    requires-python = ">=3.12"

    [[packages]]
    name = "demo"
    version = "1.0.0"
    wheels = [
        { url = "file://[TEMP_DIR]/links/demo-1.0.0-cp312-cp312-manylinux_2_17_x86_64.whl", hashes = { sha256 = "eb2ff51027ef5001a478ca15a93fbd009fdda87e36238238a52f1d4019502428" } },
        { url = "file://[TEMP_DIR]/links/demo-1.0.0-cp312-cp312-macosx_11_0_arm64.whl", hashes = { sha256 = "ed676c33c75c4e3d56b53b061173a4ec378e289013cef527ae68ce525f30be80" } },
        { url = "file://[TEMP_DIR]/links/demo-1.0.0-cp312-cp312-win_amd64.whl", hashes = { sha256 = "c0b5946665f8aebba3d880c4e5658f346e3971ec2cc0013780eab4dafbbfcad6" } },
    ]

    ----- stderr -----
    Resolved 1 package in [TIME]
    "#);
    uv_snapshot!(context.filters(), context.pip_compile().args(["pyproject.toml", "--universal", "--generate-hashes", "--offline", "--no-header", "--no-annotate"]), @r"
    exit_code: 0 (success)
    ----- stdout -----
    demo==1.0.0 \
        --hash=sha256:c0b5946665f8aebba3d880c4e5658f346e3971ec2cc0013780eab4dafbbfcad6 \
        --hash=sha256:eb2ff51027ef5001a478ca15a93fbd009fdda87e36238238a52f1d4019502428 \
        --hash=sha256:ed676c33c75c4e3d56b53b061173a4ec378e289013cef527ae68ce525f30be80

    ----- stderr -----
    warning: Setting `minimum-libc-version` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    Resolved 1 package in [TIME]
    ");
    Ok(())
}

/// Either configured libc can provide coverage, and selecting musl alone excludes GNU wheels.
#[test]
fn minimum_libc_both_families_and_musl_only() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let filters: Vec<_> = context
        .filters()
        .into_iter()
        .chain([(
            r"\nhint: The resolution failed for an environment that is not the current one[^\n]*",
            "",
        )])
        .collect();
    for (version, tag) in [
        ("1.0.0", "cp312-cp312-manylinux_2_17_x86_64"),
        ("1.0.0", "cp312-cp312-musllinux_1_2_x86_64"),
        ("2.0.0", "cp312-cp312-manylinux_2_17_x86_64"),
        ("2.0.0", "cp312-cp312-musllinux_1_3_x86_64"),
    ] {
        wheel(&context, "demo", version, tag)?;
    }

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["demo"]

        [tool.uv]
        no-index = true
        find-links = ["links"]
        environments = ["sys_platform == 'linux' and platform_machine == 'x86_64'"]
        required-environments = ["sys_platform == 'linux' and platform_machine == 'x86_64'"]
        minimum-libc-version = { glibc = "2.31", musl = "1.2" }
    "#})?;
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Setting `minimum-libc-version` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    Resolved 2 packages in [TIME]
    ");
    let lock = context.read("uv.lock");
    insta::with_settings!({filters => context.filters()}, {
        assert_snapshot!(lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        resolution-markers = [
            "platform_machine == 'x86_64' and sys_platform == 'linux'",
        ]
        supported-markers = [
            "platform_machine == 'x86_64' and sys_platform == 'linux'",
        ]
        required-markers = [
            "platform_machine == 'x86_64' and sys_platform == 'linux'",
        ]

        [options]
        minimum-libc-version = { glibc = "2.31", musl = "1.2" }
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "demo"
        version = "2.0.0"
        source = { registry = "links" }
        wheels = [
            { path = "demo-2.0.0-cp312-cp312-manylinux_2_17_x86_64.whl" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "demo" },
        ]

        [package.metadata]
        requires-dist = [{ name = "demo" }]
        "#);
    });

    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["demo"]

        [tool.uv]
        no-index = true
        find-links = ["links"]
        environments = ["sys_platform == 'linux' and platform_machine == 'x86_64'"]
        required-environments = ["sys_platform == 'linux' and platform_machine == 'x86_64'"]
        minimum-libc-version = { musl = "1.2" }
    "#})?;
    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--upgrade"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Setting `minimum-libc-version` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    Resolved 2 packages in [TIME]
    Updated demo v2.0.0 -> v1.0.0
    ");
    let lock = context.read("uv.lock");
    insta::with_settings!({filters => context.filters()}, {
        assert_snapshot!(lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        resolution-markers = [
            "platform_machine == 'x86_64' and sys_platform == 'linux'",
        ]
        supported-markers = [
            "platform_machine == 'x86_64' and sys_platform == 'linux'",
        ]
        required-markers = [
            "platform_machine == 'x86_64' and sys_platform == 'linux'",
        ]

        [options]
        minimum-libc-version = { musl = "1.2" }
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "demo"
        version = "1.0.0"
        source = { registry = "links" }
        wheels = [
            { path = "demo-1.0.0-cp312-cp312-musllinux_1_2_x86_64.whl" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "demo" },
        ]

        [package.metadata]
        requires-dist = [{ name = "demo" }]
        "#);
    });

    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["demo>=2"]

        [tool.uv]
        no-index = true
        find-links = ["links"]
        environments = ["sys_platform == 'linux' and platform_machine == 'x86_64'"]
        required-environments = ["sys_platform == 'linux' and platform_machine == 'x86_64'"]
        minimum-libc-version = { musl = "1.2" }
    "#})?;
    uv_snapshot!(filters, context.lock().arg("--offline"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    warning: Setting `minimum-libc-version` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    error: No solution found when resolving dependencies for split (markers: python_full_version >= '3.12' and platform_machine == 'x86_64' and sys_platform == 'linux')
      cause: Because demo==2.0.0 has no wheels compatible with musl 1.2 and only demo<=2.0.0 is available, we can conclude that demo>=2.0.0 cannot be used.
             And because your project depends on demo>=2, we can conclude that your project's requirements are unsatisfiable.
    ");
    Ok(())
}

/// Recompiling against an unhashed index keeps all retained artifacts' hashes.
#[test]
fn minimum_libc_unhashed_index_hashes() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let mut hashes = Vec::new();
    for tag in [
        "cp312-cp312-manylinux_2_17_x86_64",
        "cp312-cp312-manylinux_2_34_x86_64",
        "cp312-cp312-musllinux_1_2_x86_64",
    ] {
        let wheel = wheel(&context, "demo", "1.0.0", tag)?;
        hashes.push(hex::encode(Sha256::digest(fs_err::read(wheel)?)));
    }
    let context = context.with_filters([
        (hashes[0].clone(), "[GLIBC_HASH]".to_string()),
        (hashes[1].clone(), "[EXCLUDED_HASH]".to_string()),
        (hashes[2].clone(), "[MUSL_HASH]".to_string()),
    ]);
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["demo"]

        [tool.uv]
        no-index = true
        find-links = ["links"]
        minimum-libc-version = { glibc = "2.31", musl = "1.2" }
    "#})?;
    context
        .temp_dir
        .child("requirements.txt")
        .write_str(&format!(
            "demo==1.0.0 --hash=sha256:{} --hash=sha256:{} --hash=sha256:{}\n",
            hashes[0], hashes[1], hashes[2],
        ))?;

    uv_snapshot!(context.filters(), context.pip_compile().args(["pyproject.toml", "--universal", "--generate-hashes", "--offline", "--no-header", "--no-annotate", "--output-file", "requirements.txt", "--preview-features", "minimum-libc-version"]), @r"
    exit_code: 0 (success)
    ----- stdout -----
    demo==1.0.0 \
        --hash=sha256:[MUSL_HASH] \
        --hash=sha256:[GLIBC_HASH]

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    // Reuse only valid hashes on the second compile without losing either libc family.
    uv_snapshot!(context.filters(), context.pip_compile().args(["pyproject.toml", "--universal", "--generate-hashes", "--offline", "--no-header", "--no-annotate", "--output-file", "requirements.txt", "--preview-features", "minimum-libc-version"]), @r"
    exit_code: 0 (success)
    ----- stdout -----
    demo==1.0.0 \
        --hash=sha256:[MUSL_HASH] \
        --hash=sha256:[GLIBC_HASH]

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    Ok(())
}

/// Local-version fallback considers only wheels permitted by the libc cutoff.
#[test]
fn minimum_libc_local_version_fallback() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    for (version, tag) in [
        ("1.0.0", "cp312-cp312-manylinux_2_17_x86_64"),
        ("1.0.0", "cp312-cp312-manylinux_2_17_aarch64"),
        ("1.0.0+cpu", "cp312-cp312-manylinux_2_17_x86_64"),
        ("1.0.0+cpu", "cp312-cp312-manylinux_2_34_aarch64"),
    ] {
        wheel(&context, "demo", version, tag)?;
    }
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["demo; sys_platform == 'linux'"]

        [tool.uv]
        no-index = true
        find-links = ["links"]
        required-environments = []
        minimum-libc-version = { glibc = "2.31" }
    "#})?;

    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Setting `minimum-libc-version` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    Resolved 3 packages in [TIME]
    ");
    uv_snapshot!(context.filters(), context.export().args(["--frozen", "--no-hashes", "--no-header", "--no-annotate"]), @r"
        exit_code: 0 (success)
        ----- stdout -----
        demo==1.0.0 ; python_full_version < '3.13' and platform_machine == 'aarch64' and platform_python_implementation == 'CPython' and sys_platform == 'linux'
        demo==1.0.0+cpu ; (python_full_version >= '3.13' and sys_platform == 'linux') or (platform_machine != 'aarch64' and sys_platform == 'linux') or (platform_python_implementation != 'CPython' and sys_platform == 'linux')
    ");

    let lock = context.read("uv.lock");
    insta::with_settings!({filters => context.filters()}, {
        assert_snapshot!(lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        resolution-markers = [
            "python_full_version >= '3.13' or platform_machine != 'aarch64' or platform_python_implementation != 'CPython' or sys_platform != 'linux'",
            "python_full_version < '3.13' and platform_machine == 'aarch64' and platform_python_implementation == 'CPython' and sys_platform == 'linux'",
        ]

        [options]
        minimum-libc-version = { glibc = "2.31" }
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "demo"
        version = "1.0.0"
        source = { registry = "links" }
        resolution-markers = [
            "python_full_version < '3.13' and platform_machine == 'aarch64' and platform_python_implementation == 'CPython' and sys_platform == 'linux'",
        ]
        wheels = [
            { path = "demo-1.0.0-cp312-cp312-manylinux_2_17_aarch64.whl" },
        ]

        [[package]]
        name = "demo"
        version = "1.0.0+cpu"
        source = { registry = "links" }
        resolution-markers = [
            "python_full_version >= '3.13' or platform_machine != 'aarch64' or platform_python_implementation != 'CPython' or sys_platform != 'linux'",
        ]
        wheels = [
            { path = "demo-1.0.0+cpu-cp312-cp312-manylinux_2_17_x86_64.whl" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "demo", version = "1.0.0", source = { registry = "links" }, marker = "python_full_version < '3.13' and platform_machine == 'aarch64' and platform_python_implementation == 'CPython' and sys_platform == 'linux'" },
            { name = "demo", version = "1.0.0+cpu", source = { registry = "links" }, marker = "(python_full_version >= '3.13' and sys_platform == 'linux') or (platform_machine != 'aarch64' and sys_platform == 'linux') or (platform_python_implementation != 'CPython' and sys_platform == 'linux')" },
        ]

        [package.metadata]
        requires-dist = [{ name = "demo", marker = "sys_platform == 'linux'" }]
        "#);
    });
    Ok(())
}

/// Tightening the deployment floor changes the selected version and is recorded in the lock.
#[test]
fn minimum_libc_backtracks_and_invalidates_lock() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    for (version, tag) in [
        ("1.0.0", "cp312-cp312-manylinux_2_17_x86_64"),
        ("2.0.0", "cp312-cp312-manylinux_2_34_x86_64"),
    ] {
        wheel(&context, "demo", version, tag)?;
    }

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["demo"]

        [tool.uv]
        no-index = true
        find-links = ["links"]
        required-environments = ["sys_platform == 'linux' and platform_machine == 'x86_64'"]
    "#})?;
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @r"
        exit_code: 0 (success)
        ----- stderr -----
        Resolved 2 packages in [TIME]
    ");
    let original = context.read("uv.lock");

    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["demo"]

        [tool.uv]
        no-index = true
        find-links = ["links"]
        required-environments = ["sys_platform == 'linux' and platform_machine == 'x86_64'"]
        minimum-libc-version = { glibc = "2.31" }
    "#})?;
    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked"]), @"
    exit_code: 1 (failure)
    ----- stderr -----
    warning: Setting `minimum-libc-version` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    Resolved 2 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");
    assert_eq!(context.read("uv.lock"), original);
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Setting `minimum-libc-version` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    Resolved 2 packages in [TIME]
    Updated demo v2.0.0 -> v1.0.0
    ");
    uv_snapshot!(context.filters(), context.export().args(["--frozen", "--no-hashes", "--no-header", "--no-annotate"]), @r"
        exit_code: 0 (success)
        ----- stdout -----
        demo==1.0.0
    ");
    uv_snapshot!(context.filters(), context.pip_compile().args(["pyproject.toml", "--universal", "--offline", "--no-header", "--no-annotate"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    demo==1.0.0

    ----- stderr -----
    warning: Setting `minimum-libc-version` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    Resolved 1 package in [TIME]
    ");
    let lock = context.read("uv.lock");
    insta::with_settings!({filters => context.filters()}, {
        assert_snapshot!(lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        required-markers = [
            "platform_machine == 'x86_64' and sys_platform == 'linux'",
        ]

        [options]
        minimum-libc-version = { glibc = "2.31" }
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "demo"
        version = "1.0.0"
        source = { registry = "links" }
        wheels = [
            { path = "demo-1.0.0-cp312-cp312-manylinux_2_17_x86_64.whl" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "demo" },
        ]

        [package.metadata]
        requires-dist = [{ name = "demo" }]
        "#);
    });
    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Setting `minimum-libc-version` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    Resolved 2 packages in [TIME]
    ");

    // Changing the floor invalidates the lock even when the selected wheel remains compatible.
    let original = context.read("uv.lock");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["demo"]

        [tool.uv]
        no-index = true
        find-links = ["links"]
        required-environments = ["sys_platform == 'linux' and platform_machine == 'x86_64'"]
        minimum-libc-version = { glibc = "2.17" }
    "#})?;
    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked"]), @"
    exit_code: 1 (failure)
    ----- stderr -----
    warning: Setting `minimum-libc-version` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    Resolved 2 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");
    assert_eq!(context.read("uv.lock"), original);

    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["demo"]

        [tool.uv]
        no-index = true
        find-links = ["links"]
        required-environments = ["sys_platform == 'linux' and platform_machine == 'x86_64'"]
    "#})?;
    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--locked"]), @r"
        exit_code: 1 (failure)
        ----- stderr -----
        Resolved 2 packages in [TIME]
        error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

        hint: To update the lockfile, run `uv lock`.
    ");
    assert_eq!(context.read("uv.lock"), original);
    uv_snapshot!(context.filters(), context.pip_compile().args(["pyproject.toml", "--universal", "--offline", "--no-header", "--no-annotate"]), @r"
        exit_code: 0 (success)
        ----- stdout -----
        demo==2.0.0

        ----- stderr -----
        Resolved 1 package in [TIME]
    ");
    Ok(())
}

#[test]
fn minimum_libc_no_compatible_version() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let filters: Vec<_> = context
        .filters()
        .into_iter()
        .chain([(
            // This hint is only shown when the current platform doesn't match the target.
            r"\nhint: The resolution failed for an environment that is not the current one[^\n]*",
            "",
        )])
        .collect();
    for tag in [
        "cp312-cp312-manylinux_2_34_x86_64",
        "cp312-cp312-musllinux_1_2_x86_64",
    ] {
        wheel(&context, "demo", "2.0.0", tag)?;
    }
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["demo"]

        [tool.uv]
        no-index = true
        find-links = ["links"]
        required-environments = ["sys_platform == 'linux' and platform_machine == 'x86_64'"]
        minimum-libc-version = { glibc = "2.31" }
    "#})?;

    uv_snapshot!(filters, context.lock().arg("--offline"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    warning: Setting `minimum-libc-version` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    error: No solution found when resolving dependencies
      cause: Because demo==2.0.0 has no wheels compatible with glibc 2.31 and only demo==2.0.0 is available, we can conclude that all versions of demo cannot be used.
             And because your project depends on demo, we can conclude that your project's requirements are unsatisfiable.
    ");
    assert!(!context.temp_dir.child("uv.lock").exists());
    // A concrete target uses its platform tags instead of the universal libc cutoff.
    uv_snapshot!(context.filters(), context.pip_compile().args(["pyproject.toml", "--python-platform", "x86_64-manylinux_2_34", "--offline", "--no-header", "--no-annotate"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    demo==2.0.0

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    Ok(())
}

/// A source distribution remains usable when its wheel requires a newer glibc.
#[test]
fn minimum_libc_allows_sdist_fallback() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    wheel(
        &context,
        "demo",
        "2.0.0",
        "cp312-cp312-manylinux_2_34_x86_64",
    )?;
    write_tar_gz(
        fs_err::File::create(context.temp_dir.child("links/demo-2.0.0.tar.gz").path())?,
        &[
            (
                "demo-2.0.0/PKG-INFO",
                indoc! {r"
                    Metadata-Version: 2.3
                    Name: demo
                    Version: 2.0.0
                    Requires-Python: >=3.12
                "},
            ),
            (
                "demo-2.0.0/pyproject.toml",
                indoc! {r#"
                    [project]
                    name = "demo"
                    version = "2.0.0"
                    requires-python = ">=3.12"
                    dependencies = []
                "#},
            ),
        ],
    )?;
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["demo"]

        [tool.uv]
        no-index = true
        find-links = ["links"]
        required-environments = ["sys_platform == 'linux' and platform_machine == 'x86_64'"]
        minimum-libc-version = { glibc = "2.31", musl = "1.2" }
    "#})?;

    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Setting `minimum-libc-version` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    Resolved 2 packages in [TIME]
    ");
    let lock = context.read("uv.lock");
    insta::with_settings!({filters => context.filters()}, {
        assert_snapshot!(lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        required-markers = [
            "platform_machine == 'x86_64' and sys_platform == 'linux'",
        ]

        [options]
        minimum-libc-version = { glibc = "2.31", musl = "1.2" }
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "demo"
        version = "2.0.0"
        source = { registry = "links" }
        sdist = { path = "demo-2.0.0.tar.gz" }

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "demo" },
        ]

        [package.metadata]
        requires-dist = [{ name = "demo" }]
        "#);
    });

    // Reconsider the cached flat-index entry when its source distribution cannot be built.
    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--no-build", "--upgrade"]), @"
    exit_code: 1 (failure)
    ----- stderr -----
    warning: Setting `minimum-libc-version` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    error: No solution found when resolving dependencies
      cause: Because demo==2.0.0 has no usable wheels and only demo==2.0.0 is available, we can conclude that all versions of demo cannot be used.
             And because your project depends on demo, we can conclude that your project's requirements are unsatisfiable.

    hint: Wheels are required for `demo` because building from source is disabled for all packages (i.e., with `--no-build`)
    ");

    // With binaries disabled, a permitted wheel can provide metadata, but its hash must not
    // replace the source archive's hash in the compiled requirements.
    wheel(
        &context,
        "demo",
        "2.0.0",
        "cp312-cp312-manylinux_2_17_x86_64",
    )?;
    let source_hash = hex::encode(Sha256::digest(fs_err::read(
        context.temp_dir.child("links/demo-2.0.0.tar.gz"),
    )?));
    let wheel_hash = hex::encode(Sha256::digest(fs_err::read(
        context
            .temp_dir
            .child("links/demo-2.0.0-cp312-cp312-manylinux_2_17_x86_64.whl"),
    )?));
    let context = context.with_filters([
        (source_hash.clone(), "[SOURCE_HASH]".to_string()),
        (wheel_hash.clone(), "[WHEEL_HASH]".to_string()),
    ]);
    context
        .temp_dir
        .child("requirements.txt")
        .write_str(&format!(
            "demo==2.0.0 --hash=sha256:{source_hash} --hash=sha256:{wheel_hash}\n"
        ))?;
    uv_snapshot!(context.filters(), context.pip_compile().args(["pyproject.toml", "--universal", "--generate-hashes", "--offline", "--no-binary", ":all:", "--no-header", "--no-annotate", "--output-file", "requirements.txt", "--preview-features", "minimum-libc-version"]), @r"
    exit_code: 0 (success)
    ----- stdout -----
    demo==2.0.0 \
        --hash=sha256:[SOURCE_HASH]

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    Ok(())
}

#[test]
fn minimum_libc_direct_url() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let wheel = wheel(
        &context,
        "demo",
        "2.0.0",
        "cp312-cp312-manylinux_2_34_x86_64",
    )?;
    let server = FindLinksServer::new(context.temp_dir.child("links").path());
    let filename = wheel
        .file_name()
        .context("wheel has no file name")?
        .to_string_lossy();
    let dependency = format!("demo @ {}/{filename}", server.url());
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["{dependency}"]

        [tool.uv]
        no-index = true
        find-links = ["links"]
        required-environments = []
        minimum-libc-version = {{ glibc = "2.31" }}
    "#})?;

    uv_snapshot!(context.filters(), context.lock(), @"
    exit_code: 1 (failure)
    ----- stderr -----
    warning: Setting `minimum-libc-version` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    error: No solution found when resolving dependencies
      cause: Because only demo==2.0.0 is available and demo==2.0.0 has no wheels compatible with glibc 2.31, we can conclude that all versions of demo cannot be used.
             And because your project depends on demo, we can conclude that your project's requirements are unsatisfiable.
    ");
    assert!(!context.temp_dir.child("uv.lock").exists());

    // A GNU wheel admitted by the policy still cannot satisfy required musl coverage.
    pyproject_toml.write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["{dependency}"]

        [tool.uv]
        no-index = true
        required-environments = ["sys_platform == 'linux' and platform_machine == 'x86_64'"]
        minimum-libc-version = {{ glibc = "2.34", musl = "1.2" }}
    "#})?;
    uv_snapshot!(context.filters(), context.lock(), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Setting `minimum-libc-version` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    Resolved 2 packages in [TIME]
    ");
    Ok(())
}

/// Every required architecture needs coverage, while unrelated marker branches remain independent.
#[test]
fn minimum_libc_architectures_and_markers() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    for (version, tag) in [
        ("1.0.0", "cp312-cp312-manylinux2014_x86_64"),
        ("1.0.0", "cp312-cp312-manylinux_2_31_aarch64"),
        ("1.0.0", "cp312-cp312-musllinux_1_2_x86_64"),
        ("1.0.0", "cp312-cp312-musllinux_1_2_aarch64"),
        (
            "2.0.0",
            "cp312-cp312-manylinux_2_17_x86_64.manylinux_2_34_aarch64",
        ),
        ("2.0.0", "cp312-cp312-manylinux_2_34_aarch64"),
        ("2.0.0", "cp312-cp312-musllinux_1_2_aarch64"),
        ("2.0.0", "cp312-cp312-macosx_11_0_arm64"),
    ] {
        wheel(&context, "demo", version, tag)?;
    }
    wheel(&context, "windows-only", "1.0.0", "cp312-cp312-win_amd64")?;
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "demo; sys_platform == 'linux'",
            "demo>=2; sys_platform == 'darwin'",
            "windows-only; sys_platform == 'win32'",
        ]

        [tool.uv]
        no-index = true
        find-links = ["links"]
        required-environments = [
            "sys_platform == 'linux' and platform_machine == 'x86_64'",
            "sys_platform == 'linux' and platform_machine == 'aarch64'",
        ]
        minimum-libc-version = { glibc = "2.31", musl = "1.2" }
    "#})?;

    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Setting `minimum-libc-version` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    Resolved 3 packages in [TIME]
    ");
    // GNU/x86_64 and musl/aarch64 jointly cover both required architectures.
    uv_snapshot!(context.filters(), context.export().args(["--frozen", "--no-hashes", "--no-header", "--no-annotate"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    demo==2.0.0 ; sys_platform == 'darwin' or sys_platform == 'linux'
    windows-only==1.0.0 ; sys_platform == 'win32'
    ");
    let lock = context.read("uv.lock");
    insta::with_settings!({filters => context.filters()}, {
        assert_snapshot!(lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        resolution-markers = [
            "sys_platform == 'linux'",
            "sys_platform == 'darwin'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]
        required-markers = [
            "platform_machine == 'x86_64' and sys_platform == 'linux'",
            "platform_machine == 'aarch64' and sys_platform == 'linux'",
        ]

        [options]
        minimum-libc-version = { glibc = "2.31", musl = "1.2" }
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "demo"
        version = "2.0.0"
        source = { registry = "links" }
        wheels = [
            { path = "demo-2.0.0-cp312-cp312-musllinux_1_2_aarch64.whl" },
            { path = "demo-2.0.0-cp312-cp312-macosx_11_0_arm64.whl" },
            { path = "demo-2.0.0-cp312-cp312-manylinux_2_17_x86_64.manylinux_2_34_aarch64.whl" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "demo", marker = "sys_platform == 'darwin' or sys_platform == 'linux'" },
            { name = "windows-only", marker = "sys_platform == 'win32'" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "demo", marker = "sys_platform == 'darwin'", specifier = ">=2" },
            { name = "demo", marker = "sys_platform == 'linux'" },
            { name = "windows-only", marker = "sys_platform == 'win32'" },
        ]

        [[package]]
        name = "windows-only"
        version = "1.0.0"
        source = { registry = "links" }
        wheels = [
            { path = "windows_only-1.0.0-cp312-cp312-win_amd64.whl" },
        ]
        "#);
    });

    // Excluding musl removes ARM coverage from the newer version. The mixed-tag wheel
    // remains eligible for x86_64, but its too-new ARM tag cannot provide coverage.
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = [
            "demo; sys_platform == 'linux'",
            "demo>=2; sys_platform == 'darwin'",
            "windows-only; sys_platform == 'win32'",
        ]

        [tool.uv]
        no-index = true
        find-links = ["links"]
        required-environments = [
            "sys_platform == 'linux' and platform_machine == 'x86_64'",
            "sys_platform == 'linux' and platform_machine == 'aarch64'",
        ]
        minimum-libc-version = { glibc = "2.31" }
    "#})?;
    uv_snapshot!(context.filters(), context.lock().args(["--offline", "--upgrade"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Setting `minimum-libc-version` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    Resolved 4 packages in [TIME]
    Updated demo v2.0.0 -> v1.0.0, v2.0.0
    ");
    uv_snapshot!(context.filters(), context.export().args(["--frozen", "--no-hashes", "--no-header", "--no-annotate"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    demo==1.0.0 ; platform_machine == 'aarch64' and sys_platform == 'linux'
    demo==2.0.0 ; (platform_machine != 'aarch64' and sys_platform == 'linux') or sys_platform == 'darwin'
    windows-only==1.0.0 ; sys_platform == 'win32'
    ");
    Ok(())
}

#[test]
fn minimum_libc_invalid_configuration() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        no-index = true
        find-links = ["links"]
        required-environments = ["sys_platform == 'linux' and platform_machine == 'x86_64'"]
        minimum-libc-version = { glibc = "2.31.1" }
    "#})?;
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    warning: Failed to parse `pyproject.toml` during settings discovery:
      TOML parse error at line 11, column 34
         |
      11 | minimum-libc-version = { glibc = "2.31.1" }
         |                                  ^^^^^^^^
      expected a libc version in the form `<major>.<minor>` (e.g., `2.31` or `1.2`)

    error: Failed to parse: `pyproject.toml`
      cause: TOML parse error at line 11, column 34
                |
             11 | minimum-libc-version = { glibc = "2.31.1" }
                |                                  ^^^^^^^^
             expected a libc version in the form `<major>.<minor>` (e.g., `2.31` or `1.2`)
    "#);

    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        minimum-libc-version = {}
    "#})?;
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    warning: Failed to parse `pyproject.toml` during settings discovery:
      TOML parse error at line 8, column 24
        |
      8 | minimum-libc-version = {}
        |                        ^^
      at least one of `glibc` or `musl` must be specified

    error: Failed to parse: `pyproject.toml`
      cause: TOML parse error at line 8, column 24
               |
             8 | minimum-libc-version = {}
               |                        ^^
             at least one of `glibc` or `musl` must be specified
    ");

    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        minimum-libc-version = { glibc = "2.31", unknown = "1.2" }
    "#})?;
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    warning: Failed to parse `pyproject.toml` during settings discovery:
      TOML parse error at line 8, column 42
        |
      8 | minimum-libc-version = { glibc = "2.31", unknown = "1.2" }
        |                                          ^^^^^^^
      unknown field `unknown`, expected `glibc` or `musl`

    error: Failed to parse: `pyproject.toml`
      cause: TOML parse error at line 8, column 42
               |
             8 | minimum-libc-version = { glibc = "2.31", unknown = "1.2" }
               |                                          ^^^^^^^
             unknown field `unknown`, expected `glibc` or `musl`
    "#);

    // Like required-environments, the minimum libc version is a project-only setting.
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        no-index = true
        find-links = ["links"]
        required-environments = ["sys_platform == 'linux' and platform_machine == 'x86_64'"]
    "#})?;
    context.temp_dir.child("uv.toml").write_str(indoc! {r#"
        minimum-libc-version = { glibc = "2.31" }
    "#})?;
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @r"
        exit_code: 2 (failure)
        ----- stderr -----
        warning: Found both a `uv.toml` file and a `[tool.uv]` section in an adjacent `pyproject.toml`. The following fields from `[tool.uv]` will be ignored in favor of the `uv.toml` file:
        - no-index
        - find-links
        error: Failed to parse: `uv.toml`. The `minimum-libc-version` field is not allowed in a `uv.toml` file. `minimum-libc-version` is only applicable in the context of a project, and should be placed in a `pyproject.toml` file instead.
    ");
    Ok(())
}

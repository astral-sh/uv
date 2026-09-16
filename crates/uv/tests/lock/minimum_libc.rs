use std::collections::BTreeMap;

use anyhow::{Context, Result};
use assert_fs::prelude::*;
use indoc::{formatdoc, indoc};
use insta::assert_snapshot;
use sha2::{Digest, Sha256};

use uv_test::archive::write_tar_gz;
use uv_test::packse::scenario::Scenario;
use uv_test::packse::{PackseServer, generate_wheel};
use uv_test::{TestContext, uv_snapshot};

fn wheel(context: &TestContext, name: &str, version: &str, tag: &str) -> Result<()> {
    let links = context.temp_dir.child("links");
    links.create_dir_all()?;
    let (filename, bytes) = generate_wheel(
        &name.parse()?,
        &version.parse()?,
        &[],
        &BTreeMap::new(),
        None,
        tag,
        &[],
    );
    let wheel = links.child(filename);
    wheel.write_binary(&bytes)?;
    Ok(())
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
        required-environments = "sys_platform == 'linux' and platform_machine == 'x86_64'"
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
        environments = [
            { marker = "python_version >= '3.12'", libc = { glibc = "2.31" } },
        ]
        required-environments = "sys_platform == 'linux' and platform_machine == 'x86_64'"
    "#})?;
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Setting `libc` in `environments` or `required-environments` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    Resolved 2 packages in [TIME]
    ");
    let lock = context.read("uv.lock");
    insta::with_settings!({filters => context.filters()}, {
        assert_snapshot!(lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        supported-markers = [
        ]
        supported-environments = [
            { marker = "python_version >= '0'", libc = { glibc = "2.31" } },
        ]
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

    // The supported marker simplifies to true under requires-python but must remain parseable.
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
    marker = "python_full_version >= '3.12'"
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
        --hash=sha256:eb2ff51027ef5001a478ca15a93fbd009fdda87e36238238a52f1d4019502428

    ----- stderr -----
    warning: Setting `libc` in `environments` or `required-environments` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    Resolved 1 package in [TIME]
    ");
    Ok(())
}

/// Recompiling filters prior hashes after tightening libc, including when none remain eligible.
#[test]
fn minimum_libc_filters_existing_hashes() -> Result<()> {
    let scenario = toml::from_str::<Scenario>(indoc! {r#"
        name = "minimum-libc-filters-existing-hashes"

        [root]

        [expected]
        satisfiable = true

        [packages.demo.versions."1.0.0"]
        sdist = false
        wheel_tags = ["cp312-cp312-manylinux_2_17_x86_64", "cp312-cp312-manylinux_2_34_x86_64"]
    "#})?;
    let server = PackseServer::from_scenario(&scenario);
    let context = uv_test::test_context!("3.12").with_filters(
        server
            .files()
            .map(|(filename, hash)| (hash.to_owned(), format!("[SHA256:{filename}]"))),
    );
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["demo"]

        [tool.uv]
        required-environments = "sys_platform == 'linux' and platform_machine == 'x86_64'"
    "#})?;
    uv_snapshot!(context.filters(), context.pip_compile().arg("--index-url").arg(server.index_url()).args(["pyproject.toml", "--universal", "--generate-hashes", "--no-header", "--no-annotate", "--output-file", "requirements.txt"]), @r"
    exit_code: 0 (success)
    ----- stdout -----
    demo==1.0.0 \
        --hash=sha256:[SHA256:demo-1.0.0-cp312-cp312-manylinux_2_17_x86_64.whl] \
        --hash=sha256:[SHA256:demo-1.0.0-cp312-cp312-manylinux_2_34_x86_64.whl]

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");

    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["demo"]

        [tool.uv]
        environments = [
            { marker = "sys_platform == 'linux' and platform_machine == 'x86_64'", libc = { glibc = "2.31" } },
        ]
        required-environments = "sys_platform == 'linux' and platform_machine == 'x86_64'"
    "#})?;
    uv_snapshot!(context.filters(), context.pip_compile().arg("--index-url").arg(server.index_url()).args(["pyproject.toml", "--universal", "--generate-hashes", "--no-header", "--no-annotate", "--output-file", "requirements.txt", "--preview-features", "minimum-libc-version"]), @r"
    exit_code: 0 (success)
    ----- stdout -----
    demo==1.0.0 ; platform_machine == 'x86_64' and sys_platform == 'linux' \
        --hash=sha256:[SHA256:demo-1.0.0-cp312-cp312-manylinux_2_17_x86_64.whl]

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");

    // If every prior hash is excluded, replace them with hashes for eligible artifacts.
    let (_, hash) = server
        .files()
        .find(|(filename, _)| *filename == "demo-1.0.0-cp312-cp312-manylinux_2_34_x86_64.whl")
        .context("missing glibc 2.34 wheel")?;
    context
        .temp_dir
        .child("requirements.txt")
        .write_str(&format!("demo==1.0.0 --hash=sha256:{hash}\n"))?;
    uv_snapshot!(context.filters(), context.pip_compile().arg("--index-url").arg(server.index_url()).args(["pyproject.toml", "--universal", "--generate-hashes", "--no-header", "--no-annotate", "--output-file", "requirements.txt", "--preview-features", "minimum-libc-version"]), @r"
    exit_code: 0 (success)
    ----- stdout -----
    demo==1.0.0 ; platform_machine == 'x86_64' and sys_platform == 'linux' \
        --hash=sha256:[SHA256:demo-1.0.0-cp312-cp312-manylinux_2_17_x86_64.whl]

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    Ok(())
}

/// Required coverage does not discard known hashes for unhashed local index entries.
#[test]
fn minimum_libc_required_preserves_hashes() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let mut hashes = Vec::new();
    for tag in [
        "cp312-cp312-manylinux_2_17_x86_64",
        "cp312-cp312-manylinux_2_34_x86_64",
        "cp312-cp312-musllinux_1_2_x86_64",
    ] {
        wheel(&context, "demo", "1.0.0", tag)?;
        let filename = format!("demo-1.0.0-{tag}.whl");
        let hash = hex::encode(Sha256::digest(fs_err::read(
            context.temp_dir.child(format!("links/{filename}")),
        )?));
        hashes.push((filename, hash));
    }
    let context = context.with_filters(
        hashes
            .iter()
            .map(|(filename, hash)| (hash.clone(), format!("[SHA256:{filename}]"))),
    );
    context.temp_dir.child("pyproject.toml").write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["demo"]

        [tool.uv]
        no-index = true
        find-links = ["links"]
        required-environments = [
            { marker = "sys_platform == 'linux' and platform_machine == 'x86_64'", libc = { glibc = "2.31" } },
        ]
    "#})?;
    let requirements = format!(
        "demo==1.0.0 {}\n",
        hashes
            .iter()
            .map(|(_, hash)| format!("--hash=sha256:{hash}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    context
        .temp_dir
        .child("requirements.txt")
        .write_str(&requirements)?;
    uv_snapshot!(context.filters(), context.pip_compile().args(["pyproject.toml", "--universal", "--generate-hashes", "--offline", "--no-header", "--no-annotate", "--output-file", "requirements.txt", "--preview-features", "minimum-libc-version"]), @r"
    exit_code: 0 (success)
    ----- stdout -----
    demo==1.0.0 \
        --hash=sha256:[SHA256:demo-1.0.0-cp312-cp312-manylinux_2_17_x86_64.whl] \
        --hash=sha256:[SHA256:demo-1.0.0-cp312-cp312-manylinux_2_34_x86_64.whl] \
        --hash=sha256:[SHA256:demo-1.0.0-cp312-cp312-musllinux_1_2_x86_64.whl]

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    Ok(())
}

/// Required libc families need coverage; supported libc families also restrict artifacts.
#[test]
fn minimum_libc_both_families_and_musl_only() -> Result<()> {
    let scenario = toml::from_str::<Scenario>(indoc! {r#"
        name = "minimum-libc-both-families-and-musl-only"

        [root]

        [expected]
        satisfiable = true

        [packages.demo.versions."1.0.0"]
        sdist = false
        wheel_tags = ["cp312-cp312-manylinux_2_17_x86_64", "cp312-cp312-musllinux_1_2_x86_64"]

        [packages.demo.versions."2.0.0"]
        sdist = false
        wheel_tags = ["cp312-cp312-manylinux_2_17_x86_64", "cp312-cp312-musllinux_1_3_x86_64"]
    "#})?;
    let server = PackseServer::from_scenario(&scenario);
    let context = uv_test::test_context!("3.12").with_filters(
        server
            .files()
            .map(|(filename, hash)| (hash.to_owned(), format!("[SHA256:{filename}]"))),
    );
    let filters: Vec<_> = context
        .filters()
        .into_iter()
        .chain([(
            r"\nhint: The resolution failed for an environment that is not the current one[^\n]*",
            "",
        )])
        .collect();
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["demo"]

        [tool.uv]
        environments = ["sys_platform == 'linux' and platform_machine == 'x86_64'"]

        [[tool.uv.required-environments]]
        marker = "sys_platform == 'linux' and platform_machine == 'x86_64'"
        libc = { glibc = "2.31", musl = "1.2" }
    "#})?;
    uv_snapshot!(context.filters(), context.lock().arg("--index-url").arg(server.index_url()), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Setting `libc` in `environments` or `required-environments` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
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
        required-environments = [
            { marker = "platform_machine == 'x86_64' and sys_platform == 'linux'", libc = { glibc = "2.31", musl = "1.2" } },
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "demo"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        wheels = [
            { url = "http://[LOCALHOST]/files/demo-1.0.0-cp312-cp312-manylinux_2_17_x86_64.whl", hash = "sha256:[SHA256:demo-1.0.0-cp312-cp312-manylinux_2_17_x86_64.whl]", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/demo-1.0.0-cp312-cp312-musllinux_1_2_x86_64.whl", hash = "sha256:[SHA256:demo-1.0.0-cp312-cp312-musllinux_1_2_x86_64.whl]", upload-time = "2024-03-24T00:00:00Z" },
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
        environments = [
            { marker = "sys_platform == 'linux' and platform_machine == 'x86_64'", libc = { musl = "1.2" } },
        ]
        required-environments = [
            { marker = "sys_platform == 'linux' and platform_machine == 'x86_64'", libc = { musl = "1.2" } },
        ]
    "#})?;
    uv_snapshot!(context.filters(), context.lock().arg("--index-url").arg(server.index_url()).arg("--upgrade"), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Setting `libc` in `environments` or `required-environments` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
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
        supported-environments = [
            { marker = "platform_machine == 'x86_64' and sys_platform == 'linux'", libc = { musl = "1.2" } },
        ]
        required-markers = [
            "platform_machine == 'x86_64' and sys_platform == 'linux'",
        ]
        required-environments = [
            { marker = "platform_machine == 'x86_64' and sys_platform == 'linux'", libc = { musl = "1.2" } },
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "demo"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        wheels = [
            { url = "http://[LOCALHOST]/files/demo-1.0.0-cp312-cp312-musllinux_1_2_x86_64.whl", hash = "sha256:[SHA256:demo-1.0.0-cp312-cp312-musllinux_1_2_x86_64.whl]", upload-time = "2024-03-24T00:00:00Z" },
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
        environments = [
            { marker = "sys_platform == 'linux' and platform_machine == 'x86_64'", libc = { musl = "1.2" } },
        ]
        required-environments = [
            { marker = "sys_platform == 'linux' and platform_machine == 'x86_64'", libc = { musl = "1.2" } },
        ]
    "#})?;
    uv_snapshot!(filters, context.lock().arg("--index-url").arg(server.index_url()), @"
    exit_code: 1 (failure)
    ----- stderr -----
    warning: Setting `libc` in `environments` or `required-environments` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    error: No solution found when resolving dependencies for split (markers: python_full_version >= '3.12' and platform_machine == 'x86_64' and sys_platform == 'linux')
      cause: Because demo==2.0.0 has no wheels compatible with musl 1.2 for `platform_machine == 'x86_64' and sys_platform == 'linux'` and only demo<=2.0.0 is available, we can conclude that demo>=2.0.0 cannot be used.
             And because your project depends on demo>=2, we can conclude that your project's requirements are unsatisfiable.
    ");
    Ok(())
}

/// Local-version fallback considers only wheels permitted by the artifact policy.
#[test]
fn minimum_libc_local_version_fallback() -> Result<()> {
    let scenario = toml::from_str::<Scenario>(indoc! {r#"
        name = "minimum-libc-local-version-fallback"

        [root]

        [expected]
        satisfiable = true

        [packages.demo.versions."1.0.0"]
        sdist = false
        wheel_tags = ["cp312-cp312-manylinux_2_17_x86_64", "cp312-cp312-manylinux_2_17_aarch64"]

        [packages.demo.versions."1.0.0+cpu"]
        sdist = false
        wheel_tags = ["cp312-cp312-manylinux_2_17_x86_64", "cp312-cp312-manylinux_2_34_aarch64"]
    "#})?;
    let server = PackseServer::from_scenario(&scenario);
    let context = uv_test::test_context!("3.12").with_filters(
        server
            .files()
            .map(|(filename, hash)| (hash.to_owned(), format!("[SHA256:{filename}]"))),
    );
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["demo; sys_platform == 'linux'"]

        [tool.uv]
        environments = [
            { marker = "sys_platform == 'linux'", libc = { glibc = "2.31" } },
            "sys_platform != 'linux'",
        ]
        required-environments = [
            { marker = "sys_platform == 'linux'", libc = { glibc = "2.31" } },
        ]
    "#})?;

    uv_snapshot!(context.filters(), context.lock().arg("--index-url").arg(server.index_url()), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Setting `libc` in `environments` or `required-environments` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
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
            "(python_full_version >= '3.13' and sys_platform == 'linux') or (platform_machine != 'aarch64' and sys_platform == 'linux') or (platform_python_implementation != 'CPython' and sys_platform == 'linux')",
            "python_full_version < '3.13' and platform_machine == 'aarch64' and platform_python_implementation == 'CPython' and sys_platform == 'linux'",
            "sys_platform != 'linux'",
        ]
        supported-markers = [
            "sys_platform == 'linux'",
            "sys_platform != 'linux'",
        ]
        supported-environments = [
            { marker = "sys_platform == 'linux'", libc = { glibc = "2.31" } },
            "sys_platform != 'linux'",
        ]
        required-markers = [
            "sys_platform == 'linux'",
        ]
        required-environments = [
            { marker = "sys_platform == 'linux'", libc = { glibc = "2.31" } },
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "demo"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "python_full_version < '3.13' and platform_machine == 'aarch64' and platform_python_implementation == 'CPython' and sys_platform == 'linux'",
        ]
        wheels = [
            { url = "http://[LOCALHOST]/files/demo-1.0.0-cp312-cp312-manylinux_2_17_aarch64.whl", hash = "sha256:[SHA256:demo-1.0.0-cp312-cp312-manylinux_2_17_aarch64.whl]", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "demo"
        version = "1.0.0+cpu"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "(python_full_version >= '3.13' and sys_platform == 'linux') or (platform_machine != 'aarch64' and sys_platform == 'linux') or (platform_python_implementation != 'CPython' and sys_platform == 'linux')",
        ]
        wheels = [
            { url = "http://[LOCALHOST]/files/demo-1.0.0+cpu-cp312-cp312-manylinux_2_17_x86_64.whl", hash = "sha256:[SHA256:demo-1.0.0+cpu-cp312-cp312-manylinux_2_17_x86_64.whl]", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "demo", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "python_full_version < '3.13' and platform_machine == 'aarch64' and platform_python_implementation == 'CPython' and sys_platform == 'linux'" },
            { name = "demo", version = "1.0.0+cpu", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "(python_full_version >= '3.13' and sys_platform == 'linux') or (platform_machine != 'aarch64' and sys_platform == 'linux') or (platform_python_implementation != 'CPython' and sys_platform == 'linux')" },
        ]

        [package.metadata]
        requires-dist = [{ name = "demo", marker = "sys_platform == 'linux'" }]
        "#);
    });
    Ok(())
}

/// Required coverage backtracks without filtering the selected version's other artifacts.
#[test]
fn minimum_libc_backtracks_and_invalidates_lock() -> Result<()> {
    let scenario = toml::from_str::<Scenario>(indoc! {r#"
        name = "minimum-libc-backtracks-and-invalidates-lock"

        [root]

        [expected]
        satisfiable = true

        [packages.demo.versions."1.0.0"]
        sdist = false
        wheel_tags = [
            "cp312-cp312-manylinux_2_17_x86_64",
            "cp312-cp312-manylinux_2_34_x86_64",
            "cp312-cp312-musllinux_1_2_x86_64",
        ]

        [packages.demo.versions."2.0.0"]
        sdist = false
        wheel_tags = ["cp312-cp312-manylinux_2_34_x86_64"]
    "#})?;
    let server = PackseServer::from_scenario(&scenario);
    let context = uv_test::test_context!("3.12").with_filters(
        server
            .files()
            .map(|(filename, hash)| (hash.to_owned(), format!("[SHA256:{filename}]"))),
    );

    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["demo"]

        [tool.uv]
        required-environments = ["sys_platform == 'linux' and platform_machine == 'x86_64'"]
    "#})?;
    uv_snapshot!(context.filters(), context.lock().arg("--index-url").arg(server.index_url()), @r"
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
        required-environments = [
            { marker = "sys_platform == 'linux' and platform_machine == 'x86_64'", libc = { glibc = "2.31" } },
        ]
    "#})?;
    uv_snapshot!(context.filters(), context.lock().arg("--index-url").arg(server.index_url()).arg("--locked"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    warning: Setting `libc` in `environments` or `required-environments` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    Resolved 3 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");
    assert_eq!(context.read("uv.lock"), original);
    uv_snapshot!(context.filters(), context.lock().arg("--index-url").arg(server.index_url()), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Setting `libc` in `environments` or `required-environments` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    Resolved 3 packages in [TIME]
    Updated demo v2.0.0 -> v1.0.0, v2.0.0
    ");
    uv_snapshot!(context.filters(), context.export().args(["--frozen", "--no-hashes", "--no-header", "--no-annotate"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    demo==1.0.0 ; platform_machine == 'x86_64' and sys_platform == 'linux'
    demo==2.0.0 ; platform_machine != 'x86_64' or sys_platform != 'linux'
    ");
    uv_snapshot!(context.filters(), context.pip_compile().arg("--index-url").arg(server.index_url()).args(["pyproject.toml", "--universal", "--no-header", "--no-annotate"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    demo==1.0.0 ; platform_machine == 'x86_64' and sys_platform == 'linux'
    demo==2.0.0 ; platform_machine != 'x86_64' or sys_platform != 'linux'

    ----- stderr -----
    warning: Setting `libc` in `environments` or `required-environments` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    Resolved 2 packages in [TIME]
    ");
    let lock = context.read("uv.lock");
    insta::with_settings!({filters => context.filters()}, {
        assert_snapshot!(lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        resolution-markers = [
            "platform_machine != 'x86_64' or sys_platform != 'linux'",
            "platform_machine == 'x86_64' and sys_platform == 'linux'",
        ]
        required-markers = [
            "platform_machine == 'x86_64' and sys_platform == 'linux'",
        ]
        required-environments = [
            { marker = "platform_machine == 'x86_64' and sys_platform == 'linux'", libc = { glibc = "2.31" } },
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "demo"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "platform_machine == 'x86_64' and sys_platform == 'linux'",
        ]
        wheels = [
            { url = "http://[LOCALHOST]/files/demo-1.0.0-cp312-cp312-manylinux_2_17_x86_64.whl", hash = "sha256:[SHA256:demo-1.0.0-cp312-cp312-manylinux_2_17_x86_64.whl]", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/demo-1.0.0-cp312-cp312-manylinux_2_34_x86_64.whl", hash = "sha256:[SHA256:demo-1.0.0-cp312-cp312-manylinux_2_34_x86_64.whl]", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/demo-1.0.0-cp312-cp312-musllinux_1_2_x86_64.whl", hash = "sha256:[SHA256:demo-1.0.0-cp312-cp312-musllinux_1_2_x86_64.whl]", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "demo"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "platform_machine != 'x86_64' or sys_platform != 'linux'",
        ]
        wheels = [
            { url = "http://[LOCALHOST]/files/demo-2.0.0-cp312-cp312-manylinux_2_34_x86_64.whl", hash = "sha256:[SHA256:demo-2.0.0-cp312-cp312-manylinux_2_34_x86_64.whl]", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "demo", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "platform_machine == 'x86_64' and sys_platform == 'linux'" },
            { name = "demo", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "platform_machine != 'x86_64' or sys_platform != 'linux'" },
        ]

        [package.metadata]
        requires-dist = [{ name = "demo" }]
        "#);
    });
    uv_snapshot!(context.filters(), context.lock().arg("--index-url").arg(server.index_url()).arg("--locked"), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Setting `libc` in `environments` or `required-environments` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    Resolved 3 packages in [TIME]
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
        required-environments = [
            { marker = "sys_platform == 'linux' and platform_machine == 'x86_64'", libc = { glibc = "2.17" } },
        ]
    "#})?;
    uv_snapshot!(context.filters(), context.lock().arg("--index-url").arg(server.index_url()).arg("--locked"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    warning: Setting `libc` in `environments` or `required-environments` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    Resolved 3 packages in [TIME]
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
        required-environments = ["sys_platform == 'linux' and platform_machine == 'x86_64'"]
    "#})?;
    uv_snapshot!(context.filters(), context.lock().arg("--index-url").arg(server.index_url()).arg("--locked"), @r"
        exit_code: 1 (failure)
        ----- stderr -----
        Resolved 2 packages in [TIME]
        error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

        hint: To update the lockfile, run `uv lock`.
    ");
    assert_eq!(context.read("uv.lock"), original);
    uv_snapshot!(context.filters(), context.pip_compile().arg("--index-url").arg(server.index_url()).args(["pyproject.toml", "--universal", "--no-header", "--no-annotate"]), @r"
        exit_code: 0 (success)
        ----- stdout -----
        demo==2.0.0

        ----- stderr -----
        Resolved 1 package in [TIME]
    ");

    // A newer supported baseline must not conceal the stricter required coverage.
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["demo"]

        [tool.uv]
        environments = [
            { marker = "sys_platform == 'linux' and platform_machine == 'x86_64'", libc = { glibc = "2.34" } },
        ]
        required-environments = [
            { marker = "sys_platform == 'linux' and platform_machine == 'x86_64'", libc = { glibc = "2.17" } },
        ]
    "#})?;
    uv_snapshot!(context.filters(), context.pip_compile().arg("--index-url").arg(server.index_url()).args(["pyproject.toml", "--universal", "--no-header", "--no-annotate", "--preview-features", "minimum-libc-version"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    demo==1.0.0 ; platform_machine == 'x86_64' and sys_platform == 'linux'

    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    uv_snapshot!(context.filters(), context.lock().arg("--index-url").arg(server.index_url()).args(["--preview-features", "minimum-libc-version"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Updated demo v1.0.0, v2.0.0 -> v1.0.0
    ");

    // Changing only the supported baseline also invalidates the lock.
    let original = context.read("uv.lock");
    let pyproject = context.read("pyproject.toml");
    pyproject_toml.write_str(&pyproject.replace(r#"glibc = "2.34""#, r#"glibc = "2.31""#))?;
    uv_snapshot!(context.filters(), context.lock().arg("--index-url").arg(server.index_url()).args(["--locked", "--preview-features", "minimum-libc-version"]), @r"
    exit_code: 1 (failure)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: The lockfile at `uv.lock` needs to be updated, but `--locked` was provided.

    hint: To update the lockfile, run `uv lock`.
    ");
    assert_eq!(context.read("uv.lock"), original);
    Ok(())
}

#[test]
fn minimum_libc_no_compatible_version() -> Result<()> {
    let scenario = toml::from_str::<Scenario>(indoc! {r#"
        name = "minimum-libc-no-compatible-version"

        [root]

        [expected]
        satisfiable = false

        [packages.demo.versions."2.0.0"]
        sdist = false
        wheel_tags = ["cp312-cp312-manylinux_2_34_x86_64", "cp312-cp312-musllinux_1_2_x86_64"]
    "#})?;
    let server = PackseServer::from_scenario(&scenario);
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
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["demo"]

        [tool.uv]
        required-environments = [
            { marker = "sys_platform == 'linux' and platform_machine == 'x86_64'", libc = { glibc = "2.31" } },
        ]
    "#})?;

    uv_snapshot!(filters, context.lock().arg("--index-url").arg(server.index_url()), @"
    exit_code: 1 (failure)
    ----- stderr -----
    warning: Setting `libc` in `environments` or `required-environments` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    error: No solution found when resolving dependencies for split (markers: platform_machine == 'x86_64' and sys_platform == 'linux')
      cause: Because demo==2.0.0 has no `platform_machine == 'x86_64' and sys_platform == 'linux'`-compatible wheels and only demo==2.0.0 is available, we can conclude that all versions of demo cannot be used.
             And because your project depends on demo, we can conclude that your project's requirements are unsatisfiable.
    ");
    assert!(!context.temp_dir.child("uv.lock").exists());
    Ok(())
}

/// A source distribution remains usable when its wheel requires a newer glibc.
#[test]
fn minimum_libc_allows_sdist_fallback() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let filters: Vec<_> = context
        .filters()
        .into_iter()
        .chain([(
            r"\nhint: The resolution failed for an environment that is not the current one[^\n]*",
            "",
        )])
        .collect();
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
        required-environments = [
            { marker = "sys_platform == 'linux' and platform_machine == 'x86_64'", libc = { glibc = "2.31", musl = "1.2" } },
        ]
    "#})?;

    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Setting `libc` in `environments` or `required-environments` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
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
        required-environments = [
            { marker = "platform_machine == 'x86_64' and sys_platform == 'linux'", libc = { glibc = "2.31", musl = "1.2" } },
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "demo"
        version = "2.0.0"
        source = { registry = "links" }
        sdist = { path = "demo-2.0.0.tar.gz" }
        wheels = [
            { path = "demo-2.0.0-cp312-cp312-manylinux_2_34_x86_64.whl" },
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

    // Reconsider the cached flat-index entry when its source distribution cannot be built.
    uv_snapshot!(filters, context.lock().args(["--offline", "--no-build", "--upgrade"]), @"
    exit_code: 1 (failure)
    ----- stderr -----
    warning: Setting `libc` in `environments` or `required-environments` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    error: No solution found when resolving dependencies for split (markers: platform_machine == 'x86_64' and sys_platform == 'linux')
      cause: Because demo==2.0.0 has no `platform_machine == 'x86_64' and sys_platform == 'linux'`-compatible wheels and only demo==2.0.0 is available, we can conclude that all versions of demo cannot be used.
             And because your project depends on demo, we can conclude that your project's requirements are unsatisfiable.
    ");
    Ok(())
}

#[test]
fn minimum_libc_direct_url() -> Result<()> {
    let scenario = toml::from_str::<Scenario>(indoc! {r#"
        name = "minimum-libc-direct-url"

        [root]

        [expected]
        satisfiable = false

        [packages.demo.versions."2.0.0"]
        sdist = false
        wheel_tags = ["cp312-cp312-manylinux_2_34_x86_64"]
    "#})?;
    let server = PackseServer::from_scenario(&scenario);
    let context = uv_test::test_context!("3.12");
    let filters: Vec<_> = context
        .filters()
        .into_iter()
        .chain([(
            r"\nhint: The resolution failed for an environment that is not the current one[^\n]*",
            "",
        )])
        .collect();
    let dependency = format!(
        "demo @ {}",
        server.file_url("demo-2.0.0-cp312-cp312-manylinux_2_34_x86_64.whl")
    );
    let pyproject_toml = context.temp_dir.child("pyproject.toml");
    pyproject_toml.write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["{dependency}"]

        [tool.uv]
        no-index = true
        required-environments = [
            {{ marker = "sys_platform == 'linux'", libc = {{ glibc = "2.31" }} }},
        ]
    "#})?;

    uv_snapshot!(filters.clone(), context.lock(), @"
    exit_code: 1 (failure)
    ----- stderr -----
    warning: Setting `libc` in `environments` or `required-environments` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    error: No solution found when resolving dependencies
      cause: Because only demo==2.0.0 is available and demo==2.0.0 has no Linux-compatible wheels, we can conclude that all versions of demo cannot be used.
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
        required-environments = [
            {{ marker = "sys_platform == 'linux' and platform_machine == 'x86_64'", libc = {{ glibc = "2.34", musl = "1.2" }} }},
        ]
    "#})?;
    uv_snapshot!(filters.clone(), context.lock(), @"
    exit_code: 1 (failure)
    ----- stderr -----
    warning: Setting `libc` in `environments` or `required-environments` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    error: No solution found when resolving dependencies
      cause: Because only demo==2.0.0 is available and demo==2.0.0 has no `platform_machine == 'x86_64' and sys_platform == 'linux'`-compatible wheels, we can conclude that all versions of demo cannot be used.
             And because your project depends on demo, we can conclude that your project's requirements are unsatisfiable.
    ");

    // Supported environments also exclude incompatible direct wheels without a requirement.
    pyproject_toml.write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["{dependency}"]

        [tool.uv]
        no-index = true
        environments = [
            {{ marker = "sys_platform == 'linux' and platform_machine == 'x86_64'", libc = {{ glibc = "2.31" }} }},
        ]
    "#})?;
    uv_snapshot!(filters, context.lock(), @"
    exit_code: 1 (failure)
    ----- stderr -----
    warning: Setting `libc` in `environments` or `required-environments` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    error: No solution found when resolving dependencies for split (markers: platform_machine == 'x86_64' and sys_platform == 'linux')
      cause: Because only demo==2.0.0 is available and demo==2.0.0 has no wheels compatible with glibc 2.31 for `platform_machine == 'x86_64' and sys_platform == 'linux'`, we can conclude that all versions of demo cannot be used.
             And because your project depends on demo, we can conclude that your project's requirements are unsatisfiable.
    ");
    assert!(!context.temp_dir.child("uv.lock").exists());
    Ok(())
}

/// Every required architecture needs coverage, while unrelated marker branches remain independent.
#[test]
fn minimum_libc_architectures_and_markers() -> Result<()> {
    let mut scenario = toml::from_str::<Scenario>(indoc! {r#"
        name = "minimum-libc-architectures-and-markers"

        [root]

        [expected]
        satisfiable = true

        [packages.demo.versions."1.0.0"]
        sdist = false
        wheel_tags = [
            "cp312-cp312-manylinux2014_x86_64",
            "cp312-cp312-manylinux_2_31_aarch64",
            "cp312-cp312-musllinux_1_2_x86_64",
            "cp312-cp312-musllinux_1_2_aarch64",
        ]

        [packages.demo.versions."2.0.0"]
        sdist = false
        wheel_tags = [
            "cp312-cp312-manylinux_2_17_x86_64",
            "cp312-cp312-manylinux_2_34_aarch64",
            "cp312-cp312-musllinux_1_2_aarch64",
            "cp312-cp312-macosx_11_0_arm64",
        ]

        [packages.windows-only.versions."1.0.0"]
        sdist = false
        wheel_tags = ["cp312-cp312-win_amd64"]
    "#})?;
    let server = PackseServer::from_scenario(&scenario);
    let context = uv_test::test_context!("3.12").with_filters(
        server
            .files()
            .map(|(filename, hash)| (hash.to_owned(), format!("[SHA256:{filename}]"))),
    );
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
        required-environments = [
            { marker = "sys_platform == 'linux' and platform_machine == 'x86_64'", libc = { glibc = "2.31", musl = "1.2" } },
            { marker = "sys_platform == 'linux' and platform_machine == 'aarch64'", libc = { glibc = "2.31", musl = "1.2" } },
            "sys_platform == 'darwin'",
        ]
    "#})?;

    uv_snapshot!(context.filters(), context.lock().arg("--index-url").arg(server.index_url()), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Setting `libc` in `environments` or `required-environments` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    Resolved 4 packages in [TIME]
    ");
    // GNU/x86_64 and musl/aarch64 cannot jointly cover either architecture for both libc families.
    // Linux needs the older version, while the disjoint macOS branch retains the newer one.
    uv_snapshot!(context.filters(), context.export().args(["--frozen", "--no-hashes", "--no-header", "--no-annotate"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    demo==1.0.0 ; (platform_machine == 'aarch64' and sys_platform == 'linux') or (platform_machine == 'x86_64' and sys_platform == 'linux')
    demo==2.0.0 ; (platform_machine != 'aarch64' and platform_machine != 'x86_64' and sys_platform == 'linux') or sys_platform == 'darwin'
    windows-only==1.0.0 ; sys_platform == 'win32'
    ");
    let lock = context.read("uv.lock");
    insta::with_settings!({filters => context.filters()}, {
        assert_snapshot!(lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        resolution-markers = [
            "platform_machine != 'aarch64' and platform_machine != 'x86_64' and sys_platform == 'linux'",
            "platform_machine == 'aarch64' and sys_platform == 'linux'",
            "platform_machine == 'x86_64' and sys_platform == 'linux'",
            "sys_platform == 'darwin'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]
        required-markers = [
            "platform_machine == 'x86_64' and sys_platform == 'linux'",
            "platform_machine == 'aarch64' and sys_platform == 'linux'",
            "sys_platform == 'darwin'",
        ]
        required-environments = [
            { marker = "platform_machine == 'x86_64' and sys_platform == 'linux'", libc = { glibc = "2.31", musl = "1.2" } },
            { marker = "platform_machine == 'aarch64' and sys_platform == 'linux'", libc = { glibc = "2.31", musl = "1.2" } },
            "sys_platform == 'darwin'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "demo"
        version = "1.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "platform_machine == 'aarch64' and sys_platform == 'linux'",
            "platform_machine == 'x86_64' and sys_platform == 'linux'",
        ]
        wheels = [
            { url = "http://[LOCALHOST]/files/demo-1.0.0-cp312-cp312-manylinux2014_x86_64.whl", hash = "sha256:[SHA256:demo-1.0.0-cp312-cp312-manylinux2014_x86_64.whl]", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/demo-1.0.0-cp312-cp312-manylinux_2_31_aarch64.whl", hash = "sha256:[SHA256:demo-1.0.0-cp312-cp312-manylinux_2_31_aarch64.whl]", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/demo-1.0.0-cp312-cp312-musllinux_1_2_aarch64.whl", hash = "sha256:[SHA256:demo-1.0.0-cp312-cp312-musllinux_1_2_aarch64.whl]", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/demo-1.0.0-cp312-cp312-musllinux_1_2_x86_64.whl", hash = "sha256:[SHA256:demo-1.0.0-cp312-cp312-musllinux_1_2_x86_64.whl]", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "demo"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "platform_machine != 'aarch64' and platform_machine != 'x86_64' and sys_platform == 'linux'",
            "sys_platform == 'darwin'",
        ]
        wheels = [
            { url = "http://[LOCALHOST]/files/demo-2.0.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:[SHA256:demo-2.0.0-cp312-cp312-macosx_11_0_arm64.whl]", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/demo-2.0.0-cp312-cp312-manylinux_2_17_x86_64.whl", hash = "sha256:[SHA256:demo-2.0.0-cp312-cp312-manylinux_2_17_x86_64.whl]", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/demo-2.0.0-cp312-cp312-manylinux_2_34_aarch64.whl", hash = "sha256:[SHA256:demo-2.0.0-cp312-cp312-manylinux_2_34_aarch64.whl]", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/demo-2.0.0-cp312-cp312-musllinux_1_2_aarch64.whl", hash = "sha256:[SHA256:demo-2.0.0-cp312-cp312-musllinux_1_2_aarch64.whl]", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "demo", version = "1.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "(platform_machine == 'aarch64' and sys_platform == 'linux') or (platform_machine == 'x86_64' and sys_platform == 'linux')" },
            { name = "demo", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "(platform_machine != 'aarch64' and platform_machine != 'x86_64' and sys_platform == 'linux') or sys_platform == 'darwin'" },
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
        source = { registry = "http://[LOCALHOST]/simple/" }
        wheels = [
            { url = "http://[LOCALHOST]/files/windows_only-1.0.0-cp312-cp312-win_amd64.whl", hash = "sha256:[SHA256:windows_only-1.0.0-cp312-cp312-win_amd64.whl]", upload-time = "2024-03-24T00:00:00Z" },
        ]
        "#);
    });

    scenario
        .packages
        .get_mut(&"demo".parse()?)
        .context("scenario contains demo")?
        .versions
        .insert(
            "3.0.0".parse()?,
            toml::from_str(indoc! {r#"
            sdist = false
            wheel_tags = [
                "cp312-cp312-manylinux_2_31_x86_64",
                "cp312-cp312-manylinux_2_31_aarch64",
                "cp312-cp312-manylinux_2_34_s390x",
                "cp312-cp312-macosx_11_0_arm64",
            ]
        "#})?,
        );
    let server = PackseServer::from_scenario(&scenario);
    let context = context.with_filters(
        server
            .files()
            .map(|(filename, hash)| (hash.to_owned(), format!("[SHA256:{filename}]"))),
    );

    // The x86_64 floor requires a fallback; ARM accepts the newer release. Neither floor applies
    // to s390x or macOS, so those artifacts remain available for their independent branches.
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
        required-environments = [
            { marker = "sys_platform == 'linux' and platform_machine == 'x86_64'", libc = { glibc = "2.29" } },
            { marker = "sys_platform == 'linux' and platform_machine == 'aarch64'", libc = { glibc = "2.31" } },
            { marker = "sys_platform == 'darwin'" },
        ]
    "#})?;
    uv_snapshot!(context.filters(), context.lock().arg("--index-url").arg(server.index_url()).arg("--upgrade"), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: Setting `libc` in `environments` or `required-environments` is experimental and may change without warning. Pass `--preview-features minimum-libc-version` to disable this warning.
    Resolved 4 packages in [TIME]
    Updated demo v1.0.0, v2.0.0 -> v2.0.0, v3.0.0
    ");
    uv_snapshot!(context.filters(), context.export().args(["--frozen", "--no-hashes", "--no-header", "--no-annotate"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    demo==2.0.0 ; platform_machine == 'x86_64' and sys_platform == 'linux'
    demo==3.0.0 ; (platform_machine != 'x86_64' and sys_platform == 'linux') or sys_platform == 'darwin'
    windows-only==1.0.0 ; sys_platform == 'win32'
    ");
    let lock = context.read("uv.lock");
    insta::with_settings!({filters => context.filters()}, {
        assert_snapshot!(lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"
        resolution-markers = [
            "platform_machine != 'x86_64' and sys_platform == 'linux'",
            "platform_machine == 'x86_64' and sys_platform == 'linux'",
            "sys_platform == 'darwin'",
            "sys_platform != 'darwin' and sys_platform != 'linux'",
        ]
        required-markers = [
            "platform_machine == 'x86_64' and sys_platform == 'linux'",
            "platform_machine == 'aarch64' and sys_platform == 'linux'",
            "sys_platform == 'darwin'",
        ]
        required-environments = [
            { marker = "platform_machine == 'x86_64' and sys_platform == 'linux'", libc = { glibc = "2.29" } },
            { marker = "platform_machine == 'aarch64' and sys_platform == 'linux'", libc = { glibc = "2.31" } },
            "sys_platform == 'darwin'",
        ]

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "demo"
        version = "2.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "platform_machine == 'x86_64' and sys_platform == 'linux'",
        ]
        wheels = [
            { url = "http://[LOCALHOST]/files/demo-2.0.0-cp312-cp312-manylinux_2_17_x86_64.whl", hash = "sha256:[SHA256:demo-2.0.0-cp312-cp312-manylinux_2_17_x86_64.whl]", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "demo"
        version = "3.0.0"
        source = { registry = "http://[LOCALHOST]/simple/" }
        resolution-markers = [
            "platform_machine != 'x86_64' and sys_platform == 'linux'",
            "sys_platform == 'darwin'",
        ]
        wheels = [
            { url = "http://[LOCALHOST]/files/demo-3.0.0-cp312-cp312-macosx_11_0_arm64.whl", hash = "sha256:[SHA256:demo-3.0.0-cp312-cp312-macosx_11_0_arm64.whl]", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/demo-3.0.0-cp312-cp312-manylinux_2_31_aarch64.whl", hash = "sha256:[SHA256:demo-3.0.0-cp312-cp312-manylinux_2_31_aarch64.whl]", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/demo-3.0.0-cp312-cp312-manylinux_2_31_x86_64.whl", hash = "sha256:[SHA256:demo-3.0.0-cp312-cp312-manylinux_2_31_x86_64.whl]", upload-time = "2024-03-24T00:00:00Z" },
            { url = "http://[LOCALHOST]/files/demo-3.0.0-cp312-cp312-manylinux_2_34_s390x.whl", hash = "sha256:[SHA256:demo-3.0.0-cp312-cp312-manylinux_2_34_s390x.whl]", upload-time = "2024-03-24T00:00:00Z" },
        ]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "demo", version = "2.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "platform_machine == 'x86_64' and sys_platform == 'linux'" },
            { name = "demo", version = "3.0.0", source = { registry = "http://[LOCALHOST]/simple/" }, marker = "(platform_machine != 'x86_64' and sys_platform == 'linux') or sys_platform == 'darwin'" },
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
        source = { registry = "http://[LOCALHOST]/simple/" }
        wheels = [
            { url = "http://[LOCALHOST]/files/windows_only-1.0.0-cp312-cp312-win_amd64.whl", hash = "sha256:[SHA256:windows_only-1.0.0-cp312-cp312-win_amd64.whl]", upload-time = "2024-03-24T00:00:00Z" },
        ]
        "#);
    });
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
        required-environments = [
            { marker = "sys_platform == 'linux' and platform_machine == 'x86_64'", libc = { glibc = "2.31.1" } },
        ]
    "#})?;
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    warning: Failed to parse `pyproject.toml` during settings discovery:
      TOML parse error at line 11, column 93
         |
      11 |     { marker = "sys_platform == 'linux' and platform_machine == 'x86_64'", libc = { glibc = "2.31.1" } },
         |                                                                                             ^^^^^^^^
      expected a libc version in the form `<major>.<minor>` (e.g., `2.31` or `1.2`)

    error: Failed to parse: `pyproject.toml`
      cause: TOML parse error at line 11, column 93
                |
             11 |     { marker = "sys_platform == 'linux' and platform_machine == 'x86_64'", libc = { glibc = "2.31.1" } },
                |                                                                                             ^^^^^^^^
             expected a libc version in the form `<major>.<minor>` (e.g., `2.31` or `1.2`)
    "#);

    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        required-environments = [
            { marker = "sys_platform == 'linux'", libc = {} },
        ]
    "#})?;
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    warning: Failed to parse `pyproject.toml` during settings discovery:
      TOML parse error at line 9, column 50
        |
      9 |     { marker = "sys_platform == 'linux'", libc = {} },
        |                                                  ^^
      at least one of `glibc` or `musl` must be specified

    error: Failed to parse: `pyproject.toml`
      cause: TOML parse error at line 9, column 50
               |
             9 |     { marker = "sys_platform == 'linux'", libc = {} },
               |                                                  ^^
             at least one of `glibc` or `musl` must be specified
    "#);

    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        required-environments = [
            { marker = "sys_platform == 'linux'", libc = { glibc = "2.31", unknown = "1.2" } },
        ]
    "#})?;
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @r#"
    exit_code: 2 (failure)
    ----- stderr -----
    warning: Failed to parse `pyproject.toml` during settings discovery:
      TOML parse error at line 9, column 68
        |
      9 |     { marker = "sys_platform == 'linux'", libc = { glibc = "2.31", unknown = "1.2" } },
        |                                                                    ^^^^^^^
      unknown field `unknown`, expected `glibc` or `musl`

    error: Failed to parse: `pyproject.toml`
      cause: TOML parse error at line 9, column 68
               |
             9 |     { marker = "sys_platform == 'linux'", libc = { glibc = "2.31", unknown = "1.2" } },
               |                                                                    ^^^^^^^
             unknown field `unknown`, expected `glibc` or `musl`
    "#);

    // Overlap checks must include nonadjacent entries, even when their libc floors differ.
    pyproject_toml.write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = []

        [tool.uv]
        required-environments = [
            { marker = "sys_platform == 'linux' and platform_machine == 'x86_64'", libc = { glibc = "2.31" } },
            "sys_platform == 'darwin'",
            { marker = "sys_platform == 'linux' and platform_machine == 'x86_64'", libc = { glibc = "2.29" } },
        ]
    "#})?;
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Required environments must be disjoint, but the following markers overlap: `platform_machine == 'x86_64' and sys_platform == 'linux'` and `platform_machine == 'x86_64' and sys_platform == 'linux'`
    ");

    // Required environments and their libc constraints are project-only settings.
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
        required-environments = [
            { marker = "sys_platform == 'linux'", libc = { glibc = "2.31" } },
        ]
    "#})?;
    uv_snapshot!(context.filters(), context.lock().arg("--offline"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    warning: Found both a `uv.toml` file and a `[tool.uv]` section in an adjacent `pyproject.toml`. The following fields from `[tool.uv]` will be ignored in favor of the `uv.toml` file:
    - no-index
    - find-links
    error: Failed to parse: `uv.toml`. The `required-environments` field is not allowed in a `uv.toml` file. `required-environments` is only applicable in the context of a project, and should be placed in a `pyproject.toml` file instead.
    ");
    Ok(())
}

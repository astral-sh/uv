use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::Path;
#[cfg(feature = "test-git")]
use std::process::Command;
use std::process::Stdio;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[cfg(feature = "test-git")]
use anyhow::anyhow;
use anyhow::{Result, ensure};
use assert_cmd::assert::OutputAssertExt;
use assert_fs::fixture::ChildPath;
use assert_fs::prelude::*;
use indoc::{formatdoc, indoc};
use insta::assert_snapshot;
use predicates::prelude::predicate;
use serde_json::json;
use sha2::{Digest, Sha256};
#[cfg(feature = "test-git")]
use url::Url;
use uv_distribution_types::BuildInfo;
use uv_lock::{BuildOperation, BuildStage, Lock};
use uv_test::packse::{generate_sdist_with_files, generate_wheel_with_files};
use uv_test::{TestContext, apply_filters, diff_snapshot, uv_snapshot};
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A backend that records the selected build dependency in the wheel it produces.
const BUILD_BACKEND: &str = indoc! {r#"
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
"#};

/// Write a deterministic wheel using the shared test-package generator.
fn write_wheel(
    directory: &ChildPath,
    name: &str,
    version: &str,
    requires: &[&str],
    files: &[(&str, &str)],
) -> Result<()> {
    let requirements = requires
        .iter()
        .map(|requirement| requirement.parse())
        .collect::<Result<Vec<_>, _>>()?;
    let (filename, wheel) = generate_wheel_with_files(
        &name.parse()?,
        &version.parse()?,
        &requirements,
        &BTreeMap::new(),
        None,
        "py3-none-any",
        files,
    );
    let path = directory.child(filename);
    path.write_binary(&wheel)?;
    Ok(())
}

/// Build-only packages available from a local wheel directory.
fn build_wheels(context: &TestContext) -> Result<ChildPath> {
    let wheels = context.temp_dir.child("wheels");
    wheels.create_dir_all()?;
    write_wheel(&wheels, "helper", "1.0.0", &[], &[])?;
    write_wheel(&wheels, "helper", "2.0.0", &[], &[])?;
    write_wheel(&wheels, "dynamic-helper", "1.0.0", &[], &[])?;
    write_wheel(&wheels, "runtime-only", "1.0.0", &[], &[])?;
    write_wheel(
        &wheels,
        "builder",
        "1.0.0",
        &["helper>=1,<3"],
        &[("builder/backend.py", BUILD_BACKEND)],
    )?;
    Ok(wheels)
}

fn write_source(directory: &ChildPath, name: &str, helper: &str) -> Result<()> {
    directory.create_dir_all()?;
    directory
        .child("pyproject.toml")
        .write_str(&formatdoc! {r#"
        [project]
        name = "{name}"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["runtime-only==1.0.0"]

        [build-system]
        requires = ["builder==1.0.0", "helper=={helper}"]
        build-backend = "builder.backend"
    "#})?;
    Ok(())
}

/// A source project whose backend and dependencies are available without an index.
fn build_lock_project() -> Result<(TestContext, ChildPath)> {
    let context = uv_test::test_context!("3.12");
    let wheels = build_wheels(&context)?;
    write_source(&context.temp_dir, "project", "1.0.0")?;
    Ok((context, wheels))
}

/// Two source packages that require incompatible versions of the same build dependency.
fn build_lock_workspace() -> Result<(TestContext, ChildPath)> {
    let context = uv_test::test_context!("3.12");
    let wheels = build_wheels(&context)?;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["first", "second"]

        [tool.uv.sources]
        first = { path = "first" }
        second = { path = "second" }
    "#})?;
    write_source(&context.temp_dir.child("first"), "first", "1.0.0")?;
    write_source(&context.temp_dir.child("second"), "second", "2.0.0")?;
    Ok((context, wheels))
}

#[test]
fn build_dependencies_preview_is_explicit() -> Result<()> {
    let (context, wheels) = build_lock_project()?;

    uv_snapshot!(context.filters(), context.lock()
        .args(["--preview", "--offline", "--no-index", "--find-links"])
        .arg(wheels.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
    Resolved 2 packages in [TIME]
    ");
    let ordinary = context.read("uv.lock");
    insta::with_settings!({filters => context.filters()}, {
        assert_snapshot!(ordinary, @r#"
        version = 1
        revision = 4
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "runtime-only" },
        ]

        [[package]]
        name = "runtime-only"
        version = "1.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        wheels = [
            { path = "[TEMP_DIR]/wheels/runtime_only-1.0.0-py3-none-any.whl" },
        ]
        "#);
    });

    uv_snapshot!(context.filters(), context.lock().arg("--build-dependencies"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: `uv lock --build-dependencies` requires `--preview-features build-dependency-locking`
    ");
    assert_eq!(context.read("uv.lock"), ordinary);

    uv_snapshot!(context.filters(), context.lock()
        .args(["--build-dependencies", "--preview-features", "build-dependency-locking",
            "--offline", "--no-index", "--find-links"])
        .arg(wheels.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    uv_snapshot!(context.filters(), context.lock()
        .args(["--no-build-dependencies", "--offline", "--no-index", "--find-links"])
        .arg(wheels.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    insta::with_settings!({filters => context.filters()}, {
        assert_snapshot!(context.read("uv.lock"), @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { editable = "." }
        dependencies = [
            { name = "runtime-only" },
        ]

        [package.metadata]
        requires-dist = [{ name = "runtime-only", specifier = "==1.0.0" }]

        [[package]]
        name = "runtime-only"
        version = "1.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        wheels = [
            { path = "[TEMP_DIR]/wheels/runtime_only-1.0.0-py3-none-any.whl" },
        ]
        "#);
    });
    Ok(())
}

#[test]
fn build_dependencies_independent_graphs() -> Result<()> {
    let (context, wheels) = build_lock_workspace()?;
    let context = context.with_filter((
        r#"(implementation_version|os_name|platform_machine|platform_release|platform_system|platform_version|python_full_version|sys_platform) = "[^"]*""#,
        "$1 = \"[EXECUTOR]\"",
    ));

    uv_snapshot!(context.filters(), context.lock()
        .args(["--build-dependencies", "--preview-features", "build-dependency-locking",
            "--offline", "--no-index", "--find-links"])
        .arg(wheels.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 4 packages in [TIME]
    ");
    insta::with_settings!({filters => context.filters()}, {
        assert_snapshot!(context.read("uv.lock"), @r#"
        version = 2
        revision = 3
        requires-python = ">=3.12"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [[package]]
        name = "first"
        version = "0.1.0"
        source = { directory = "first" }
        dependencies = [
            { name = "runtime-only" },
        ]

        [package.metadata]
        requires-dist = [{ name = "runtime-only", specifier = "==1.0.0" }]

        [[package]]
        name = "project"
        version = "0.1.0"
        source = { virtual = "." }
        dependencies = [
            { name = "first" },
            { name = "second" },
        ]

        [package.metadata]
        requires-dist = [
            { name = "first", directory = "first" },
            { name = "second", directory = "second" },
        ]

        [[package]]
        name = "runtime-only"
        version = "1.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        wheels = [
            { path = "[TEMP_DIR]/wheels/runtime_only-1.0.0-py3-none-any.whl" },
        ]

        [[package]]
        name = "second"
        version = "0.1.0"
        source = { directory = "second" }
        dependencies = [
            { name = "runtime-only" },
        ]

        [package.metadata]
        requires-dist = [{ name = "runtime-only", specifier = "==1.0.0" }]

        [build-lock]
        executor = { marker-environment = { implementation_name = "cpython", implementation_version = "[EXECUTOR]", os_name = "[EXECUTOR]", platform_machine = "[EXECUTOR]", platform_python_implementation = "CPython", platform_release = "[EXECUTOR]", platform_system = "[EXECUTOR]", platform_version = "[EXECUTOR]", python_full_version = "[EXECUTOR]", python_version = "3.12", sys_platform = "[EXECUTOR]" }, abi-tag = "cp312" }

        [[build-lock.resolution]]
        name = "first"
        source = { directory = "first" }
        operation = "wheel"
        input = { kind = "pyproject", hash = { algorithm = "Sha256", digest = "dfda5d6476bd7504689681ad3c8f00bbc410adf007231895bbcc628e697c8b2d" } }
        declared-requirements = [
            { name = "builder", specifier = "==1.0.0" },
            { name = "helper", specifier = "==1.0.0" },
        ]
        backend-requirements = [
            { name = "dynamic-helper", specifier = "==1.0.0" },
        ]

        [build-lock.resolution.bootstrap]
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [build-lock.resolution.bootstrap.options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [build-lock.resolution.bootstrap.manifest]
        requirements = [
            { name = "builder", specifier = "==1.0.0" },
            { name = "helper", specifier = "==1.0.0" },
        ]

        [[build-lock.resolution.bootstrap.package]]
        name = "builder"
        version = "1.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        dependencies = [
            { name = "helper" },
        ]
        wheels = [
            { path = "[TEMP_DIR]/wheels/builder-1.0.0-py3-none-any.whl", hash = "sha256:48f386f449f59eb0c260248f1ed7c82eb9b6cc5f263e5d55c40d25c3ea8e7fed" },
        ]

        [[build-lock.resolution.bootstrap.package]]
        name = "helper"
        version = "1.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        wheels = [
            { path = "[TEMP_DIR]/wheels/helper-1.0.0-py3-none-any.whl", hash = "sha256:8e4243504d8d0640496e8ed29c485bc6063c738abbb25b9d028b1c9376bcdc56" },
        ]

        [build-lock.resolution.final]
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [build-lock.resolution.final.options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [build-lock.resolution.final.manifest]
        requirements = [
            { name = "builder", specifier = "==1.0.0" },
            { name = "dynamic-helper", specifier = "==1.0.0" },
            { name = "helper", specifier = "==1.0.0" },
        ]

        [[build-lock.resolution.final.package]]
        name = "builder"
        version = "1.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        dependencies = [
            { name = "helper" },
        ]
        wheels = [
            { path = "[TEMP_DIR]/wheels/builder-1.0.0-py3-none-any.whl", hash = "sha256:48f386f449f59eb0c260248f1ed7c82eb9b6cc5f263e5d55c40d25c3ea8e7fed" },
        ]

        [[build-lock.resolution.final.package]]
        name = "dynamic-helper"
        version = "1.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        wheels = [
            { path = "[TEMP_DIR]/wheels/dynamic_helper-1.0.0-py3-none-any.whl", hash = "sha256:acd43933e16294195b440c23b4b131b0b3bb2784ce6c45a9cbb8a83a6065acc1" },
        ]

        [[build-lock.resolution.final.package]]
        name = "helper"
        version = "1.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        wheels = [
            { path = "[TEMP_DIR]/wheels/helper-1.0.0-py3-none-any.whl", hash = "sha256:8e4243504d8d0640496e8ed29c485bc6063c738abbb25b9d028b1c9376bcdc56" },
        ]

        [[build-lock.resolution]]
        name = "first"
        source = { directory = "first" }
        operation = "editable"
        input = { kind = "pyproject", hash = { algorithm = "Sha256", digest = "dfda5d6476bd7504689681ad3c8f00bbc410adf007231895bbcc628e697c8b2d" } }
        declared-requirements = [
            { name = "builder", specifier = "==1.0.0" },
            { name = "helper", specifier = "==1.0.0" },
        ]
        backend-requirements = [
            { name = "dynamic-helper", specifier = "==1.0.0" },
        ]

        [build-lock.resolution.bootstrap]
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [build-lock.resolution.bootstrap.options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [build-lock.resolution.bootstrap.manifest]
        requirements = [
            { name = "builder", specifier = "==1.0.0" },
            { name = "helper", specifier = "==1.0.0" },
        ]

        [[build-lock.resolution.bootstrap.package]]
        name = "builder"
        version = "1.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        dependencies = [
            { name = "helper" },
        ]
        wheels = [
            { path = "[TEMP_DIR]/wheels/builder-1.0.0-py3-none-any.whl", hash = "sha256:48f386f449f59eb0c260248f1ed7c82eb9b6cc5f263e5d55c40d25c3ea8e7fed" },
        ]

        [[build-lock.resolution.bootstrap.package]]
        name = "helper"
        version = "1.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        wheels = [
            { path = "[TEMP_DIR]/wheels/helper-1.0.0-py3-none-any.whl", hash = "sha256:8e4243504d8d0640496e8ed29c485bc6063c738abbb25b9d028b1c9376bcdc56" },
        ]

        [build-lock.resolution.final]
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [build-lock.resolution.final.options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [build-lock.resolution.final.manifest]
        requirements = [
            { name = "builder", specifier = "==1.0.0" },
            { name = "dynamic-helper", specifier = "==1.0.0" },
            { name = "helper", specifier = "==1.0.0" },
        ]

        [[build-lock.resolution.final.package]]
        name = "builder"
        version = "1.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        dependencies = [
            { name = "helper" },
        ]
        wheels = [
            { path = "[TEMP_DIR]/wheels/builder-1.0.0-py3-none-any.whl", hash = "sha256:48f386f449f59eb0c260248f1ed7c82eb9b6cc5f263e5d55c40d25c3ea8e7fed" },
        ]

        [[build-lock.resolution.final.package]]
        name = "dynamic-helper"
        version = "1.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        wheels = [
            { path = "[TEMP_DIR]/wheels/dynamic_helper-1.0.0-py3-none-any.whl", hash = "sha256:acd43933e16294195b440c23b4b131b0b3bb2784ce6c45a9cbb8a83a6065acc1" },
        ]

        [[build-lock.resolution.final.package]]
        name = "helper"
        version = "1.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        wheels = [
            { path = "[TEMP_DIR]/wheels/helper-1.0.0-py3-none-any.whl", hash = "sha256:8e4243504d8d0640496e8ed29c485bc6063c738abbb25b9d028b1c9376bcdc56" },
        ]

        [[build-lock.resolution]]
        name = "second"
        source = { directory = "second" }
        operation = "wheel"
        input = { kind = "pyproject", hash = { algorithm = "Sha256", digest = "3ef74aa884038d73b5c297c8b53a2b37dc36fecf5e2dc3a0765f9e4ecad34163" } }
        declared-requirements = [
            { name = "builder", specifier = "==1.0.0" },
            { name = "helper", specifier = "==2.0.0" },
        ]
        backend-requirements = [
            { name = "dynamic-helper", specifier = "==1.0.0" },
        ]

        [build-lock.resolution.bootstrap]
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [build-lock.resolution.bootstrap.options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [build-lock.resolution.bootstrap.manifest]
        requirements = [
            { name = "builder", specifier = "==1.0.0" },
            { name = "helper", specifier = "==2.0.0" },
        ]

        [[build-lock.resolution.bootstrap.package]]
        name = "builder"
        version = "1.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        dependencies = [
            { name = "helper" },
        ]
        wheels = [
            { path = "[TEMP_DIR]/wheels/builder-1.0.0-py3-none-any.whl", hash = "sha256:48f386f449f59eb0c260248f1ed7c82eb9b6cc5f263e5d55c40d25c3ea8e7fed" },
        ]

        [[build-lock.resolution.bootstrap.package]]
        name = "helper"
        version = "2.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        wheels = [
            { path = "[TEMP_DIR]/wheels/helper-2.0.0-py3-none-any.whl", hash = "sha256:50b83ea38e9b3bad35aa22be2346403b9207d48e54099e6e0ea2f29893dbbe92" },
        ]

        [build-lock.resolution.final]
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [build-lock.resolution.final.options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [build-lock.resolution.final.manifest]
        requirements = [
            { name = "builder", specifier = "==1.0.0" },
            { name = "dynamic-helper", specifier = "==1.0.0" },
            { name = "helper", specifier = "==2.0.0" },
        ]

        [[build-lock.resolution.final.package]]
        name = "builder"
        version = "1.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        dependencies = [
            { name = "helper" },
        ]
        wheels = [
            { path = "[TEMP_DIR]/wheels/builder-1.0.0-py3-none-any.whl", hash = "sha256:48f386f449f59eb0c260248f1ed7c82eb9b6cc5f263e5d55c40d25c3ea8e7fed" },
        ]

        [[build-lock.resolution.final.package]]
        name = "dynamic-helper"
        version = "1.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        wheels = [
            { path = "[TEMP_DIR]/wheels/dynamic_helper-1.0.0-py3-none-any.whl", hash = "sha256:acd43933e16294195b440c23b4b131b0b3bb2784ce6c45a9cbb8a83a6065acc1" },
        ]

        [[build-lock.resolution.final.package]]
        name = "helper"
        version = "2.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        wheels = [
            { path = "[TEMP_DIR]/wheels/helper-2.0.0-py3-none-any.whl", hash = "sha256:50b83ea38e9b3bad35aa22be2346403b9207d48e54099e6e0ea2f29893dbbe92" },
        ]

        [[build-lock.resolution]]
        name = "second"
        source = { directory = "second" }
        operation = "editable"
        input = { kind = "pyproject", hash = { algorithm = "Sha256", digest = "3ef74aa884038d73b5c297c8b53a2b37dc36fecf5e2dc3a0765f9e4ecad34163" } }
        declared-requirements = [
            { name = "builder", specifier = "==1.0.0" },
            { name = "helper", specifier = "==2.0.0" },
        ]
        backend-requirements = [
            { name = "dynamic-helper", specifier = "==1.0.0" },
        ]

        [build-lock.resolution.bootstrap]
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [build-lock.resolution.bootstrap.options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [build-lock.resolution.bootstrap.manifest]
        requirements = [
            { name = "builder", specifier = "==1.0.0" },
            { name = "helper", specifier = "==2.0.0" },
        ]

        [[build-lock.resolution.bootstrap.package]]
        name = "builder"
        version = "1.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        dependencies = [
            { name = "helper" },
        ]
        wheels = [
            { path = "[TEMP_DIR]/wheels/builder-1.0.0-py3-none-any.whl", hash = "sha256:48f386f449f59eb0c260248f1ed7c82eb9b6cc5f263e5d55c40d25c3ea8e7fed" },
        ]

        [[build-lock.resolution.bootstrap.package]]
        name = "helper"
        version = "2.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        wheels = [
            { path = "[TEMP_DIR]/wheels/helper-2.0.0-py3-none-any.whl", hash = "sha256:50b83ea38e9b3bad35aa22be2346403b9207d48e54099e6e0ea2f29893dbbe92" },
        ]

        [build-lock.resolution.final]
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [build-lock.resolution.final.options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [build-lock.resolution.final.manifest]
        requirements = [
            { name = "builder", specifier = "==1.0.0" },
            { name = "dynamic-helper", specifier = "==1.0.0" },
            { name = "helper", specifier = "==2.0.0" },
        ]

        [[build-lock.resolution.final.package]]
        name = "builder"
        version = "1.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        dependencies = [
            { name = "helper" },
        ]
        wheels = [
            { path = "[TEMP_DIR]/wheels/builder-1.0.0-py3-none-any.whl", hash = "sha256:48f386f449f59eb0c260248f1ed7c82eb9b6cc5f263e5d55c40d25c3ea8e7fed" },
        ]

        [[build-lock.resolution.final.package]]
        name = "dynamic-helper"
        version = "1.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        wheels = [
            { path = "[TEMP_DIR]/wheels/dynamic_helper-1.0.0-py3-none-any.whl", hash = "sha256:acd43933e16294195b440c23b4b131b0b3bb2784ce6c45a9cbb8a83a6065acc1" },
        ]

        [[build-lock.resolution.final.package]]
        name = "helper"
        version = "2.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        wheels = [
            { path = "[TEMP_DIR]/wheels/helper-2.0.0-py3-none-any.whl", hash = "sha256:50b83ea38e9b3bad35aa22be2346403b9207d48e54099e6e0ea2f29893dbbe92" },
        ]
        "#);
    });

    // Only the wheel files and lockfile are available to the new cache.
    let context = context.with_cache_dir("fresh-cache");
    uv_snapshot!(context.filters(), context.sync()
        .args(["--frozen", "--offline", "--no-index", "--no-editable"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Prepared 3 packages in [TIME]
    Installed 3 packages in [TIME]
     + first==0.1.0 (from file://[TEMP_DIR]/first)
     + runtime-only==1.0.0
     + second==0.1.0 (from file://[TEMP_DIR]/second)
    ");
    uv_snapshot!(context.filters(), context.run().args([
        "--no-sync", "python", "-c",
        "import first, second, importlib.util; print(first.selected, second.selected); print(importlib.util.find_spec('builder'))",
    ]), @"
    exit_code: 0 (success)
    ----- stdout -----
    1.0.0 2.0.0
    None
    ");

    let lock = Lock::from_toml(&context.read("uv.lock"))?;
    let receipt: BuildInfo = serde_json::from_str(&fs_err::read_to_string(
        context
            .site_packages()
            .join("first-0.1.0.dist-info/uv_build.json"),
    )?)?;
    assert_eq!(
        receipt.build_lock_fingerprint(),
        Some(&lock.build_lock().expect("build contract").fingerprint()?),
    );
    Ok(())
}

#[test]
fn build_dependencies_changed_declarations() -> Result<()> {
    let (context, wheels) = build_lock_project()?;
    uv_snapshot!(context.filters(), context.lock()
        .args(["--build-dependencies", "--preview-features", "build-dependency-locking",
            "--offline", "--no-index", "--find-links"])
        .arg(wheels.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    uv_snapshot!(context.filters(), context.sync()
        .args(["--frozen", "--offline", "--no-index", "--no-editable"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + runtime-only==1.0.0
    ");
    let original = context.read("uv.lock");

    write_source(&context.temp_dir, "project", "2.0.0")?;
    uv_snapshot!(context.filters(), context.sync()
        .args(["--frozen", "--offline", "--no-index", "--no-editable"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The build declarations for `project @ directory+.` changed; update the build lock
    ");
    assert_eq!(context.read("uv.lock"), original);

    uv_snapshot!(context.filters(), context.lock()
        .args(["--build-dependencies", "--preview-features", "build-dependency-locking",
            "--offline", "--no-index", "--find-links"])
        .arg(wheels.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    insta::with_settings!({filters => context.filters()}, {
        assert_snapshot!(diff_snapshot(&original, &context.read("uv.lock"), 3), @r#"
        --- old
        +++ new
        @@ -31,10 +31,10 @@
         name = "project"
         source = { directory = "." }
         operation = "wheel"
        -input = { kind = "pyproject", hash = { algorithm = "Sha256", digest = "94522970c3d2c144abed92cc1593ccf2373cec5d84a0e003b724aede4d564c06" } }
        +input = { kind = "pyproject", hash = { algorithm = "Sha256", digest = "68e29e5538647ed82c7ab4d362de9c1267cf054205209804ab17f553f9e54526" } }
         declared-requirements = [
             { name = "builder", specifier = "==1.0.0" },
        -    { name = "helper", specifier = "==1.0.0" },
        +    { name = "helper", specifier = "==2.0.0" },
         ]
         backend-requirements = [
             { name = "dynamic-helper", specifier = "==1.0.0" },
        @@ -51,7 +51,7 @@
         [build-lock.resolution.bootstrap.manifest]
         requirements = [
             { name = "builder", specifier = "==1.0.0" },
        -    { name = "helper", specifier = "==1.0.0" },
        +    { name = "helper", specifier = "==2.0.0" },
         ]

         [[build-lock.resolution.bootstrap.package]]
        @@ -67,10 +67,10 @@

         [[build-lock.resolution.bootstrap.package]]
         name = "helper"
        -version = "1.0.0"
        +version = "2.0.0"
         source = { registry = "[TEMP_DIR]/wheels" }
         wheels = [
        -    { path = "[TEMP_DIR]/wheels/helper-1.0.0-py3-none-any.whl", hash = "sha256:8e4243504d8d0640496e8ed29c485bc6063c738abbb25b9d028b1c9376bcdc56" },
        +    { path = "[TEMP_DIR]/wheels/helper-2.0.0-py3-none-any.whl", hash = "sha256:50b83ea38e9b3bad35aa22be2346403b9207d48e54099e6e0ea2f29893dbbe92" },
         ]

         [build-lock.resolution.final]
        @@ -85,7 +85,7 @@
         requirements = [
             { name = "builder", specifier = "==1.0.0" },
             { name = "dynamic-helper", specifier = "==1.0.0" },
        -    { name = "helper", specifier = "==1.0.0" },
        +    { name = "helper", specifier = "==2.0.0" },
         ]

         [[build-lock.resolution.final.package]]
        @@ -109,20 +109,20 @@

         [[build-lock.resolution.final.package]]
         name = "helper"
        -version = "1.0.0"
        +version = "2.0.0"
         source = { registry = "[TEMP_DIR]/wheels" }
         wheels = [
        -    { path = "[TEMP_DIR]/wheels/helper-1.0.0-py3-none-any.whl", hash = "sha256:8e4243504d8d0640496e8ed29c485bc6063c738abbb25b9d028b1c9376bcdc56" },
        +    { path = "[TEMP_DIR]/wheels/helper-2.0.0-py3-none-any.whl", hash = "sha256:50b83ea38e9b3bad35aa22be2346403b9207d48e54099e6e0ea2f29893dbbe92" },
         ]

         [[build-lock.resolution]]
         name = "project"
         source = { directory = "." }
         operation = "editable"
        -input = { kind = "pyproject", hash = { algorithm = "Sha256", digest = "94522970c3d2c144abed92cc1593ccf2373cec5d84a0e003b724aede4d564c06" } }
        +input = { kind = "pyproject", hash = { algorithm = "Sha256", digest = "68e29e5538647ed82c7ab4d362de9c1267cf054205209804ab17f553f9e54526" } }
         declared-requirements = [
             { name = "builder", specifier = "==1.0.0" },
        -    { name = "helper", specifier = "==1.0.0" },
        +    { name = "helper", specifier = "==2.0.0" },
         ]
         backend-requirements = [
             { name = "dynamic-helper", specifier = "==1.0.0" },
        @@ -139,7 +139,7 @@
         [build-lock.resolution.bootstrap.manifest]
         requirements = [
             { name = "builder", specifier = "==1.0.0" },
        -    { name = "helper", specifier = "==1.0.0" },
        +    { name = "helper", specifier = "==2.0.0" },
         ]

         [[build-lock.resolution.bootstrap.package]]
        @@ -155,10 +155,10 @@

         [[build-lock.resolution.bootstrap.package]]
         name = "helper"
        -version = "1.0.0"
        +version = "2.0.0"
         source = { registry = "[TEMP_DIR]/wheels" }
         wheels = [
        -    { path = "[TEMP_DIR]/wheels/helper-1.0.0-py3-none-any.whl", hash = "sha256:8e4243504d8d0640496e8ed29c485bc6063c738abbb25b9d028b1c9376bcdc56" },
        +    { path = "[TEMP_DIR]/wheels/helper-2.0.0-py3-none-any.whl", hash = "sha256:50b83ea38e9b3bad35aa22be2346403b9207d48e54099e6e0ea2f29893dbbe92" },
         ]

         [build-lock.resolution.final]
        @@ -173,7 +173,7 @@
         requirements = [
             { name = "builder", specifier = "==1.0.0" },
             { name = "dynamic-helper", specifier = "==1.0.0" },
        -    { name = "helper", specifier = "==1.0.0" },
        +    { name = "helper", specifier = "==2.0.0" },
         ]

         [[build-lock.resolution.final.package]]
        @@ -197,8 +197,8 @@

         [[build-lock.resolution.final.package]]
         name = "helper"
        -version = "1.0.0"
        +version = "2.0.0"
         source = { registry = "[TEMP_DIR]/wheels" }
         wheels = [
        -    { path = "[TEMP_DIR]/wheels/helper-1.0.0-py3-none-any.whl", hash = "sha256:8e4243504d8d0640496e8ed29c485bc6063c738abbb25b9d028b1c9376bcdc56" },
        +    { path = "[TEMP_DIR]/wheels/helper-2.0.0-py3-none-any.whl", hash = "sha256:50b83ea38e9b3bad35aa22be2346403b9207d48e54099e6e0ea2f29893dbbe92" },
         ]
        "#);
    });
    uv_snapshot!(context.filters(), context.sync()
        .args(["--frozen", "--offline", "--no-index", "--no-editable"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Prepared 1 package in [TIME]
    Uninstalled 1 package in [TIME]
    Installed 1 package in [TIME]
     ~ project==0.1.0 (from file://[TEMP_DIR]/)
    ");
    uv_snapshot!(context.filters(), context.run()
        .args(["--no-sync", "python", "-c", "import project; print(project.selected)"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    2.0.0
    ");
    Ok(())
}

#[test]
fn build_dependencies_reconcile_final_environment() -> Result<()> {
    let (context, wheels) = build_lock_project()?;
    write_wheel(&wheels, "bootstrap-only", "1.0.0", &[], &[])?;
    write_wheel(&wheels, "helper", "2.0.0", &["bootstrap-only==1.0.0"], &[])?;
    write_wheel(
        &wheels,
        "switching-builder",
        "1.0.0",
        &["builder==1.0.0"],
        &[(
            "switching_builder/backend.py",
            indoc! {r#"
            import importlib.util
            from builder.backend import build_wheel as _build_wheel

            def get_requires_for_build_wheel(config_settings=None):
                assert importlib.util.find_spec("bootstrap_only") is not None
                return ["dynamic-helper==1.0.0", "helper==1.0.0"]

            get_requires_for_build_editable = get_requires_for_build_wheel

            def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
                assert importlib.util.find_spec("bootstrap_only") is None
                return _build_wheel(wheel_directory, config_settings, metadata_directory)

            build_editable = build_wheel
        "#},
        )],
    )?;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"

        [build-system]
        requires = ["switching-builder==1.0.0"]
        build-backend = "switching_builder.backend"
    "#})?;

    uv_snapshot!(context.filters(), context.lock()
        .args(["--build-dependencies", "--preview-features", "build-dependency-locking",
            "--offline", "--no-index", "--find-links"])
        .arg(wheels.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    let lock = Lock::from_toml(&context.read("uv.lock"))?;
    let build = lock
        .build_lock()
        .expect("build contract")
        .resolutions()
        .iter()
        .find(|build| build.operation() == BuildOperation::Wheel)
        .expect("wheel build");
    let bootstrap = build
        .graph(BuildStage::Bootstrap)
        .expect("bootstrap graph")
        .to_toml()?;
    let final_graph = build
        .graph(BuildStage::Final)
        .expect("final graph")
        .to_toml()?;
    insta::with_settings!({filters => context.filters()}, {
        assert_snapshot!(bootstrap, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [{ name = "switching-builder", specifier = "==1.0.0" }]

        [[package]]
        name = "bootstrap-only"
        version = "1.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        wheels = [
            { path = "[TEMP_DIR]/wheels/bootstrap_only-1.0.0-py3-none-any.whl", hash = "sha256:2e9df847d6ba7bd1a1d0f5b307f49ab762179751e26f32aff47a50ff9300d810" },
        ]

        [[package]]
        name = "builder"
        version = "1.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        dependencies = [
            { name = "helper" },
        ]
        wheels = [
            { path = "[TEMP_DIR]/wheels/builder-1.0.0-py3-none-any.whl", hash = "sha256:48f386f449f59eb0c260248f1ed7c82eb9b6cc5f263e5d55c40d25c3ea8e7fed" },
        ]

        [[package]]
        name = "helper"
        version = "2.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        dependencies = [
            { name = "bootstrap-only" },
        ]
        wheels = [
            { path = "[TEMP_DIR]/wheels/helper-2.0.0-py3-none-any.whl", hash = "sha256:4f54244ef8155ddf37d3bee46369a8f3d70247ff8554dbf25b263a7bdadfde94" },
        ]

        [[package]]
        name = "switching-builder"
        version = "1.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        dependencies = [
            { name = "builder" },
        ]
        wheels = [
            { path = "[TEMP_DIR]/wheels/switching_builder-1.0.0-py3-none-any.whl", hash = "sha256:2bbbb6c4a834280efdf3d11d674421855670cfe9425aea2eca5688c36d375935" },
        ]
        "#);
        assert_snapshot!(final_graph, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12.[X]"

        [options]
        exclude-newer = "2024-03-25T00:00:00Z"

        [manifest]
        requirements = [
            { name = "dynamic-helper", specifier = "==1.0.0" },
            { name = "helper", specifier = "==1.0.0" },
            { name = "switching-builder", specifier = "==1.0.0" },
        ]

        [[package]]
        name = "builder"
        version = "1.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        dependencies = [
            { name = "helper" },
        ]
        wheels = [
            { path = "[TEMP_DIR]/wheels/builder-1.0.0-py3-none-any.whl", hash = "sha256:48f386f449f59eb0c260248f1ed7c82eb9b6cc5f263e5d55c40d25c3ea8e7fed" },
        ]

        [[package]]
        name = "dynamic-helper"
        version = "1.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        wheels = [
            { path = "[TEMP_DIR]/wheels/dynamic_helper-1.0.0-py3-none-any.whl", hash = "sha256:acd43933e16294195b440c23b4b131b0b3bb2784ce6c45a9cbb8a83a6065acc1" },
        ]

        [[package]]
        name = "helper"
        version = "1.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        wheels = [
            { path = "[TEMP_DIR]/wheels/helper-1.0.0-py3-none-any.whl", hash = "sha256:8e4243504d8d0640496e8ed29c485bc6063c738abbb25b9d028b1c9376bcdc56" },
        ]

        [[package]]
        name = "switching-builder"
        version = "1.0.0"
        source = { registry = "[TEMP_DIR]/wheels" }
        dependencies = [
            { name = "builder" },
        ]
        wheels = [
            { path = "[TEMP_DIR]/wheels/switching_builder-1.0.0-py3-none-any.whl", hash = "sha256:2bbbb6c4a834280efdf3d11d674421855670cfe9425aea2eca5688c36d375935" },
        ]
        "#);
    });

    let context = context.with_cache_dir("fresh-cache");
    uv_snapshot!(context.filters(), context.sync()
        .args(["--frozen", "--offline", "--no-index", "--no-editable"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/)
    ");
    uv_snapshot!(context.filters(), context.run()
        .args(["--no-sync", "python", "-c", "import project; print(project.selected)"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    1.0.0
    ");
    Ok(())
}

#[test]
fn build_dependencies_missing_coverage_fails_before_reuse() -> Result<()> {
    let (context, wheels) = build_lock_project()?;
    uv_snapshot!(context.filters(), context.lock()
        .args(["--build-dependencies", "--preview-features", "build-dependency-locking",
            "--offline", "--no-index", "--find-links"])
        .arg(wheels.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    uv_snapshot!(context.filters(), context.sync()
        .args(["--frozen", "--offline", "--no-index", "--no-editable"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + runtime-only==1.0.0
    ");

    let mut lock = toml::from_str::<toml::Value>(&context.read("uv.lock"))?;
    lock["build-lock"]["resolution"]
        .as_array_mut()
        .expect("build resolutions")
        .retain(|resolution| resolution["operation"].as_str() != Some("wheel"));
    context
        .temp_dir
        .child("uv.lock")
        .write_str(&toml::to_string(&lock)?)?;

    uv_snapshot!(context.filters(), context.sync()
        .args(["--frozen", "--offline", "--no-index", "--no-editable"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The build lock does not cover the wheel build of `project @ directory+.`
    ");
    Ok(())
}

#[test]
fn build_dependencies_executor_mismatch_fails_before_reuse() -> Result<()> {
    let (context, wheels) = build_lock_project()?;
    uv_snapshot!(context.filters(), context.lock()
        .args(["--build-dependencies", "--preview-features", "build-dependency-locking",
            "--offline", "--no-index", "--find-links"])
        .arg(wheels.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    uv_snapshot!(context.filters(), context.sync()
        .args(["--frozen", "--offline", "--no-index", "--no-editable"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + project==0.1.0 (from file://[TEMP_DIR]/)
     + runtime-only==1.0.0
    ");

    let mut lock = toml::from_str::<toml::Value>(&context.read("uv.lock"))?;
    lock["build-lock"]["executor"]["abi-tag"] = toml::Value::String("different".to_owned());
    context
        .temp_dir
        .child("uv.lock")
        .write_str(&toml::to_string(&lock)?)?;

    uv_snapshot!(context.filters(), context.sync()
        .args(["--frozen", "--offline", "--no-index", "--no-editable"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The build lock does not cover this executor
    ");
    Ok(())
}

#[test]
fn build_dependencies_reject_unrecorded_settings() -> Result<()> {
    let (context, wheels) = build_lock_project()?;
    uv_snapshot!(context.filters(), context.lock()
        .args(["--build-dependencies", "--preview-features", "build-dependency-locking",
            "--offline", "--no-index", "--find-links"])
        .arg(wheels.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    uv_snapshot!(context.filters(), context.sync()
        .args(["--frozen", "--offline", "--no-index", "--no-build-isolation"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Build dependency locking requires build isolation
    ");
    uv_snapshot!(context.filters(), context.sync()
        .args(["--frozen", "--offline", "--no-index", "--config-setting", "key=value"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Build dependency locking does not yet support config settings, extra build dependencies, or extra build variables
    ");
    Ok(())
}

#[test]
fn build_dependencies_reject_export() -> Result<()> {
    let (context, wheels) = build_lock_project()?;
    uv_snapshot!(context.filters(), context.lock()
        .args(["--build-dependencies", "--preview-features", "build-dependency-locking",
            "--offline", "--no-index", "--find-links"])
        .arg(wheels.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    uv_snapshot!(context.filters(), context.export().args(["--frozen", "--offline"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: `uv export` cannot represent the required build-dependency contract; remove it explicitly with `uv lock --no-build-dependencies` before exporting
    ");
    Ok(())
}

#[test]
fn build_dependencies_reject_run_with() -> Result<()> {
    let (context, wheels) = build_lock_project()?;
    uv_snapshot!(context.filters(), context.lock()
        .args(["--build-dependencies", "--preview-features", "build-dependency-locking",
            "--offline", "--no-index", "--find-links"])
        .arg(wheels.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    uv_snapshot!(context.filters(), context.run().args([
        "--frozen", "--with", "runtime-only==1.0.0", "python", "-c", "pass",
    ]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: A project build lock does not yet support ephemeral `--with` requirements
    ");
    uv_snapshot!(context.filters(), context.run().args([
        "--no-sync", "--with", "runtime-only==1.0.0", "python", "-c", "pass",
    ]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: A project build lock does not yet support ephemeral `--with` requirements
    ");
    uv_snapshot!(context.filters(), context.run().args([
        "--no-sync", "--isolated", "--with", "runtime-only==1.0.0", "python", "-c", "pass",
    ]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: A project build lock does not yet support ephemeral `--with` requirements
    ");
    Ok(())
}

#[test]
fn build_dependencies_replaced_wheel_fails_hash_verification() -> Result<()> {
    let (context, wheels) = build_lock_project()?;
    let builder = wheels.child("builder-1.0.0-py3-none-any.whl");
    let original_hash = hex::encode(Sha256::digest(fs_err::read(&builder)?));
    uv_snapshot!(context.filters(), context.lock()
        .args(["--build-dependencies", "--preview-features", "build-dependency-locking",
            "--offline", "--no-index", "--find-links"])
        .arg(wheels.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    let original = context.read("uv.lock");

    write_wheel(
        &wheels,
        "builder",
        "1.0.0",
        &["helper>=1,<3"],
        &[(
            "builder/backend.py",
            "raise RuntimeError('replaced backend executed')\n",
        )],
    )?;
    let changed_hash = hex::encode(Sha256::digest(fs_err::read(&builder)?));
    let context = context
        .with_cache_dir("fresh-cache")
        .with_filter((original_hash, "[BUILD_HASH]"))
        .with_filter((changed_hash, "[CHANGED_BUILD_HASH]"));
    uv_snapshot!(context.filters(), context.sync()
        .args(["--frozen", "--offline", "--no-index", "--no-editable"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `project @ file://[TEMP_DIR]/`
      cause: Failed to install requirements from `build-system.requires`
      cause: Failed to download `builder==1.0.0`
      cause: Hash mismatch for `builder==1.0.0`

             Expected:
               sha256:[BUILD_HASH]

             Computed:
               sha256:[CHANGED_BUILD_HASH]
    ");
    assert_eq!(context.read("uv.lock"), original);
    Ok(())
}

#[test]
fn build_dependencies_build_wheel() -> Result<()> {
    let (context, wheels) = build_lock_project()?;
    uv_snapshot!(context.filters(), context.lock()
        .args(["--build-dependencies", "--preview-features", "build-dependency-locking",
            "--offline", "--no-index", "--find-links"])
        .arg(wheels.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");

    let context = context.with_cache_dir("fresh-build-cache");
    uv_snapshot!(context.filters(), context.build()
        .args(["--wheel", "--offline", "--no-index"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    warning: The cache directory `fresh-build-cache` is inside the build source directory `.` and may be included in distributions
    Building wheel...
    Successfully built dist/project-0.1.0-py3-none-any.whl
    ");
    context
        .temp_dir
        .child("dist/project-0.1.0-py3-none-any.whl")
        .assert(predicate::path::exists());
    Ok(())
}

#[test]
fn build_dependencies_build_rejects_unsupported_operations_before_clear() -> Result<()> {
    let (context, wheels) = build_lock_project()?;
    uv_snapshot!(context.filters(), context.lock()
        .args(["--build-dependencies", "--preview-features", "build-dependency-locking",
            "--offline", "--no-index", "--find-links"])
        .arg(wheels.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    let keep = context.temp_dir.child("dist/keep.txt");
    keep.write_str("keep")?;

    uv_snapshot!(context.filters(), context.build().arg("--clear"), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: A project build lock supports `uv build --wheel` from a locked source directory; source distribution and file-list builds are not yet supported
    ");
    keep.assert("keep");
    uv_snapshot!(context.filters(), context.build().args(["--sdist", "--clear"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: A project build lock supports `uv build --wheel` from a locked source directory; source distribution and file-list builds are not yet supported
    ");
    keep.assert("keep");
    uv_snapshot!(context.filters(), context.build().args(["--wheel", "--list", "--clear"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: A project build lock supports `uv build --wheel` from a locked source directory; source distribution and file-list builds are not yet supported
    ");
    keep.assert("keep");
    uv_snapshot!(context.filters(), context.build()
        .args(["--wheel", "--no-build-isolation", "--clear"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/`
      cause: Build dependency locking requires build isolation
    ");
    keep.assert("keep");
    Ok(())
}

#[test]
fn build_dependencies_build_rejects_changed_source_before_clear() -> Result<()> {
    let (context, wheels) = build_lock_project()?;
    uv_snapshot!(context.filters(), context.lock()
        .args(["--build-dependencies", "--preview-features", "build-dependency-locking",
            "--offline", "--no-index", "--find-links"])
        .arg(wheels.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    let original = context.read("uv.lock");
    let keep = context.temp_dir.child("dist/keep.txt");
    keep.write_str("keep")?;

    write_source(&context.temp_dir, "project", "2.0.0")?;
    uv_snapshot!(context.filters(), context.build()
        .args(["--wheel", "--offline", "--no-index", "--clear"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to build `[TEMP_DIR]/`
      cause: The build declarations for `project @ directory+.` changed; update the build lock
    ");
    keep.assert("keep");

    fs_err::remove_file(context.temp_dir.child("pyproject.toml"))?;
    context
        .temp_dir
        .child("setup.py")
        .write_str("raise RuntimeError('unlocked legacy build')\n")?;
    uv_snapshot!(context.filters(), context.build()
        .args(["--wheel", "--offline", "--no-index", "--clear"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The source directory has a required build lock, but its workspace could not be discovered
    ");
    keep.assert("keep");
    assert_eq!(context.read("uv.lock"), original);
    Ok(())
}

#[test]
fn build_dependencies_upgrade_constraints_and_preferences() -> Result<()> {
    let (context, wheels) = build_lock_project()?;
    let pyproject = context.temp_dir.child("pyproject.toml");
    pyproject.write_str(
        &context
            .read("pyproject.toml")
            .replace("helper==1.0.0", "helper>=1,<3"),
    )?;

    uv_snapshot!(context.filters(), context.lock()
        .args(["--build-dependencies", "--preview-features", "build-dependency-locking",
            "--upgrade-package", "helper==1.0.0", "--offline", "--no-index", "--find-links"])
        .arg(wheels.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    let original = context.read("uv.lock");
    uv_snapshot!(context.filters(), context.lock()
        .args(["--build-dependencies", "--preview-features", "build-dependency-locking",
            "--offline", "--no-index", "--find-links"])
        .arg(wheels.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    assert_eq!(context.read("uv.lock"), original);

    uv_snapshot!(context.filters(), context.lock()
        .args(["--upgrade-package", "helper==2.0.0", "--offline", "--no-index", "--find-links"])
        .arg(wheels.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    let upgraded = context.read("uv.lock");
    insta::with_settings!({filters => context.filters()}, {
        assert_snapshot!(diff_snapshot(&original, &upgraded, 3), @r#"
        --- old
        +++ new
        @@ -67,10 +67,10 @@

         [[build-lock.resolution.bootstrap.package]]
         name = "helper"
        -version = "1.0.0"
        +version = "2.0.0"
         source = { registry = "[TEMP_DIR]/wheels" }
         wheels = [
        -    { path = "[TEMP_DIR]/wheels/helper-1.0.0-py3-none-any.whl", hash = "sha256:8e4243504d8d0640496e8ed29c485bc6063c738abbb25b9d028b1c9376bcdc56" },
        +    { path = "[TEMP_DIR]/wheels/helper-2.0.0-py3-none-any.whl", hash = "sha256:50b83ea38e9b3bad35aa22be2346403b9207d48e54099e6e0ea2f29893dbbe92" },
         ]

         [build-lock.resolution.final]
        @@ -109,10 +109,10 @@

         [[build-lock.resolution.final.package]]
         name = "helper"
        -version = "1.0.0"
        +version = "2.0.0"
         source = { registry = "[TEMP_DIR]/wheels" }
         wheels = [
        -    { path = "[TEMP_DIR]/wheels/helper-1.0.0-py3-none-any.whl", hash = "sha256:8e4243504d8d0640496e8ed29c485bc6063c738abbb25b9d028b1c9376bcdc56" },
        +    { path = "[TEMP_DIR]/wheels/helper-2.0.0-py3-none-any.whl", hash = "sha256:50b83ea38e9b3bad35aa22be2346403b9207d48e54099e6e0ea2f29893dbbe92" },
         ]

         [[build-lock.resolution]]
        @@ -155,10 +155,10 @@

         [[build-lock.resolution.bootstrap.package]]
         name = "helper"
        -version = "1.0.0"
        +version = "2.0.0"
         source = { registry = "[TEMP_DIR]/wheels" }
         wheels = [
        -    { path = "[TEMP_DIR]/wheels/helper-1.0.0-py3-none-any.whl", hash = "sha256:8e4243504d8d0640496e8ed29c485bc6063c738abbb25b9d028b1c9376bcdc56" },
        +    { path = "[TEMP_DIR]/wheels/helper-2.0.0-py3-none-any.whl", hash = "sha256:50b83ea38e9b3bad35aa22be2346403b9207d48e54099e6e0ea2f29893dbbe92" },
         ]

         [build-lock.resolution.final]
        @@ -197,8 +197,8 @@

         [[build-lock.resolution.final.package]]
         name = "helper"
        -version = "1.0.0"
        +version = "2.0.0"
         source = { registry = "[TEMP_DIR]/wheels" }
         wheels = [
        -    { path = "[TEMP_DIR]/wheels/helper-1.0.0-py3-none-any.whl", hash = "sha256:8e4243504d8d0640496e8ed29c485bc6063c738abbb25b9d028b1c9376bcdc56" },
        +    { path = "[TEMP_DIR]/wheels/helper-2.0.0-py3-none-any.whl", hash = "sha256:50b83ea38e9b3bad35aa22be2346403b9207d48e54099e6e0ea2f29893dbbe92" },
         ]
        "#);
    });
    uv_snapshot!(context.filters(), context.lock()
        .args(["--upgrade-package", "helper==3.0.0", "--offline", "--no-index", "--find-links"])
        .arg(wheels.path()), @"
    exit_code: 2 (failure)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    error: Failed to resolve requirements from `build-system.requires`
      cause: No solution found when resolving: `builder==1.0.0`, `helper>=1, <3`
      cause: Because you require helper>=1,<3 and helper==3.0.0, we can conclude that your requirements are unsatisfiable.
    ");
    assert_eq!(context.read("uv.lock"), upgraded);
    Ok(())
}

#[test]
fn build_dependencies_reject_nested_source_without_publishing() -> Result<()> {
    let (context, wheels) = build_lock_project()?;
    write_wheel(
        &wheels,
        "builder",
        "1.0.0",
        &["helper>=1,<3", "nested==1.0.0"],
        &[("builder/backend.py", BUILD_BACKEND)],
    )?;
    let (filename, archive) = generate_sdist_with_files(
        &"nested".parse()?,
        &"1.0.0".parse()?,
        &[
            (
                "pyproject.toml",
                indoc! {r#"
                [build-system]
                requires = []
                build-backend = "backend"
                backend-path = ["."]
            "#},
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
    wheels.child(filename).write_binary(&archive)?;

    uv_snapshot!(context.filters(), context.lock()
        .args(["--offline", "--no-index", "--find-links"]).arg(wheels.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    let original = context.read("uv.lock");
    uv_snapshot!(context.filters(), context.lock()
        .args(["--build-dependencies", "--preview-features", "build-dependency-locking",
            "--offline", "--no-index", "--find-links"])
        .arg(wheels.path()), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to resolve requirements from `build-system.requires`
      cause: No solution found when resolving: `builder==1.0.0`, `helper==1.0.0`
      cause: Because nested==1.0.0 has no usable wheels and all versions of builder depend on nested==1.0.0, we can conclude that all versions of builder cannot be used.
             And because you require builder==1.0.0, we can conclude that your requirements are unsatisfiable.

    hint: Wheels are required for `nested` because building from source is disabled for all packages (i.e., with `--no-build`)
    ");
    assert_eq!(context.read("uv.lock"), original);
    Ok(())
}

#[test]
fn build_dependencies_reject_dynamic_runtime_metadata() -> Result<()> {
    let (context, wheels) = build_lock_project()?;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&context.read("pyproject.toml").replace(
            "dependencies = [\"runtime-only==1.0.0\"]",
            "dynamic = [\"dependencies\"]",
        ))?;

    uv_snapshot!(context.filters(), context.lock()
        .args(["--offline", "--no-index", "--find-links"]).arg(wheels.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 1 package in [TIME]
    ");
    let original = context.read("uv.lock");
    uv_snapshot!(context.filters(), context.lock()
        .args(["--build-dependencies", "--preview-features", "build-dependency-locking",
            "--offline", "--no-index", "--find-links"])
        .arg(wheels.path()), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Build requirement discovery for `project @ file://[TEMP_DIR]/` requires static source metadata
    ");
    assert_eq!(context.read("uv.lock"), original);
    Ok(())
}

#[test]
fn build_dependencies_reject_script_without_mutation() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let script = context.temp_dir.child("example.py");
    script.write_str("print('hello')\n")?;

    uv_snapshot!(context.filters(), context.lock().args([
        "--script", "example.py", "--build-dependencies", "--preview-features",
        "build-dependency-locking",
    ]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Build dependency locking is not yet supported for scripts
    ");
    script.assert("print('hello')\n");
    context
        .temp_dir
        .child("example.py.lock")
        .assert(predicate::path::missing());
    Ok(())
}

#[test]
fn build_dependencies_reject_tool_installation() -> Result<()> {
    let (context, wheels) = build_lock_project()?;
    let context = context.with_tool_dirs();
    uv_snapshot!(context.filters(), context.lock()
        .args(["--build-dependencies", "--preview-features", "build-dependency-locking",
            "--offline", "--no-index", "--find-links"])
        .arg(wheels.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    let original = context.read("uv.lock");
    let tool_lock = context.temp_dir.child("tools/helper/uv.lock");
    tool_lock.write_str(&original)?;

    uv_snapshot!(context.filters(), context.tool_install()
        .args(["helper==1.0.0", "--force", "--offline", "--no-index", "--find-links"])
        .arg(wheels.path()), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The tool lockfile at `tools/helper/uv.lock` requires build dependency locking, which tool installation does not support
    ");
    uv_snapshot!(context.filters(), context.tool_install()
        .args(["helper==1.0.0", "--force", "--preview-features", "tool-install-locks",
            "--offline", "--no-index", "--find-links"])
        .arg(wheels.path()), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: The tool lockfile at `tools/helper/uv.lock` requires build dependency locking, which tool installation does not support
    ");
    uv_snapshot!(context.filters(), context.tool_upgrade().arg("helper"), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: Failed to upgrade helper
      cause: The tool lockfile at `tools/helper/uv.lock` requires build dependency locking, which tool installation does not support
    ");
    uv_snapshot!(context.filters(), context.tool_upgrade()
        .args(["helper", "--preview-features", "tool-install-locks"]), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: Failed to upgrade helper
      cause: The tool lockfile at `tools/helper/uv.lock` requires build dependency locking, which tool installation does not support
    ");
    tool_lock.assert(original);
    Ok(())
}

#[test]
fn build_dependencies_path_archive() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let wheels = build_wheels(&context)?;
    let source = context.temp_dir.child("source");
    write_source(&source, "archive", "1.0.0")?;
    let pyproject = context.read("source/pyproject.toml");
    let (filename, archive) = generate_sdist_with_files(
        &"archive".parse()?,
        &"0.1.0".parse()?,
        &[("pyproject.toml", &pyproject)],
    );
    context.temp_dir.child(&filename).write_binary(&archive)?;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["archive"]

        [tool.uv.sources]
        archive = {{ path = "{filename}" }}
    "#})?;

    uv_snapshot!(context.filters(), context.lock()
        .args(["--build-dependencies", "--preview-features", "build-dependency-locking",
            "--offline", "--no-index", "--find-links"])
        .arg(wheels.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");
    let context = context.with_cache_dir("fresh-cache");
    uv_snapshot!(context.filters(), context.sync().args(["--frozen", "--offline", "--no-index"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + archive==0.1.0 (from file://[TEMP_DIR]/archive-0.1.0.tar.gz)
     + runtime-only==1.0.0
    ");
    uv_snapshot!(context.filters(), context.run()
        .args(["--no-sync", "python", "-c", "import archive; print(archive.selected)"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    1.0.0
    ");
    Ok(())
}

#[cfg(feature = "test-git")]
#[test]
fn build_dependencies_git_source() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let wheels = build_wheels(&context)?;
    let repository = context.temp_dir.child("repository");
    write_source(&repository.child("package"), "git-source", "2.0.0")?;
    Command::new("git")
        .arg("init")
        .arg(repository.path())
        .assert()
        .success();
    Command::new("git")
        .arg("-C")
        .arg(repository.path())
        .args(["add", "."])
        .assert()
        .success();
    Command::new("git")
        .arg("-C")
        .arg(repository.path())
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
    let repository_url = Url::from_directory_path(repository.path())
        .map_err(|()| anyhow!("invalid repository path"))?;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&formatdoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["git-source"]

        [tool.uv.sources]
        git-source = {{ git = "{repository_url}", subdirectory = "package" }}
    "#})?;

    uv_snapshot!(context.filters(), context.lock()
        .args(["--build-dependencies", "--preview-features", "build-dependency-locking",
            "--offline", "--no-index", "--find-links"])
        .arg(wheels.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");
    let context = context.with_cache_dir("fresh-git-cache");
    uv_snapshot!(context.filters(), context.sync().args(["--frozen", "--offline", "--no-index"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + git-source==0.1.0 (from git+file://[TEMP_DIR]/repository/@0a280c8429dec9c3453ceb532d8dbac58f4e3ef4#subdirectory=package)
     + runtime-only==1.0.0
    ");
    uv_snapshot!(context.filters(), context.run()
        .args(["--no-sync", "python", "-c", "import git_source; print(git_source.selected)"]), @"
    exit_code: 0 (success)
    ----- stdout -----
    2.0.0
    ");
    Ok(())
}

#[tokio::test]
async fn build_dependencies_hashless_source_archive() -> Result<()> {
    let context = uv_test::test_context!("3.12");
    let wheels = build_wheels(&context)?;
    write_source(&context.temp_dir.child("source"), "archive", "1.0.0")?;
    let pyproject = context.read("source/pyproject.toml");
    let (filename, archive) = generate_sdist_with_files(
        &"archive".parse()?,
        &"0.1.0".parse()?,
        &[
            ("pyproject.toml", &pyproject),
            (
                "PKG-INFO",
                "Metadata-Version: 2.3\nName: archive\nVersion: 0.1.0\nRequires-Python: >=3.12\nRequires-Dist: runtime-only==1.0.0\n",
            ),
        ],
    );
    let hash = hex::encode(Sha256::digest(&archive));
    let server = MockServer::start().await;
    let archive_path = format!("/files/{filename}");
    Mock::given(path("/simple/archive/")).respond_with(
        ResponseTemplate::new(200).set_body_raw(json!({
            "meta": {"api-version": "1.0"},
            "name": "archive",
            "files": [{"filename": filename, "url": archive_path, "hashes": {}, "upload-time": "2024-01-01T00:00:00Z"}]
        }).to_string(), "application/vnd.pypi.simple.v1+json"),
    ).mount(&server).await;
    Mock::given(path(&archive_path))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(archive))
        .mount(&server)
        .await;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(indoc! {r#"
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.12"
        dependencies = ["archive==0.1.0"]
    "#})?;

    uv_snapshot!(context.filters(), context.lock()
        .args(["--build-dependencies", "--preview-features", "build-dependency-locking", "--index-url"])
        .arg(format!("{}/simple", server.uri())).arg("--find-links").arg(wheels.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 3 packages in [TIME]
    ");
    let original = context.read("uv.lock");
    let mut lock = toml::from_str::<toml::Value>(&original)?;
    let source = lock["package"]
        .as_array_mut()
        .expect("packages")
        .iter_mut()
        .find(|package| package["name"].as_str() == Some("archive"))
        .expect("source package");
    assert_eq!(
        source["sdist"]["hash"].as_str(),
        Some(format!("sha256:{hash}").as_str())
    );

    // The required contract must not survive removal of its selected source hash.
    source["sdist"]
        .as_table_mut()
        .expect("sdist")
        .remove("hash");
    context
        .temp_dir
        .child("uv.lock")
        .write_str(&toml::to_string(&lock)?)?;
    uv_snapshot!(context.filters(), context.sync().args(["--frozen", "--no-index"]), @"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to parse `uv.lock`
      cause: Invalid build lock: a covered source archive is missing its hash
    ");
    context.temp_dir.child("uv.lock").write_str(&original)?;
    uv_snapshot!(context.filters(), context.sync().args(["--frozen", "--no-index"]), @"
    exit_code: 0 (success)
    ----- stderr -----
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + archive==0.1.0
     + runtime-only==1.0.0
    ");

    let (_, replaced) = generate_sdist_with_files(
        &"archive".parse()?,
        &"0.1.0".parse()?,
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
    let changed_hash = hex::encode(Sha256::digest(&replaced));
    server.reset().await;
    Mock::given(path(&archive_path))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(replaced))
        .mount(&server)
        .await;
    let context = context
        .with_cache_dir("replaced-source-cache")
        .with_filter((hash, "[SOURCE_HASH]"))
        .with_filter((changed_hash, "[CHANGED_SOURCE_HASH]"));
    uv_snapshot!(context.filters(), context.sync()
        .args(["--frozen", "--no-index", "--reinstall"]), @"
    exit_code: 1 (failure)
    ----- stderr -----
    error: Failed to download and build `archive==0.1.0`
      cause: Hash mismatch for `archive==0.1.0`

             Expected:
               sha256:[SOURCE_HASH]

             Computed:
               sha256:[CHANGED_SOURCE_HASH]

    hint: `archive` (v0.1.0) was included because `project` (v0.1.0) depends on `archive`
    ");
    assert_eq!(context.read("uv.lock"), original);
    Ok(())
}

/// A source project whose backend waits for the test to release it.
fn gated_build_lock_project() -> Result<(TestContext, ChildPath)> {
    let (context, wheels) = build_lock_project()?;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&context.read("pyproject.toml").replace(
            "build-backend = \"builder.backend\"",
            "build-backend = \"backend\"\nbackend-path = [\".\"]",
        ))?;
    context.temp_dir.child("backend.py").write_str(indoc! {r#"
        import time
        from pathlib import Path
        from builder.backend import build_wheel, build_editable
        from builder.backend import get_requires_for_build_wheel as _get_requires

        def get_requires_for_build_wheel(config_settings=None):
            Path("gate.ready").touch()
            deadline = time.monotonic() + 45
            while not Path("gate.release").exists():
                assert time.monotonic() < deadline, "build lock test gate timed out"
                time.sleep(0.01)
            return _get_requires(config_settings)

        get_requires_for_build_editable = get_requires_for_build_wheel
    "#})?;
    Ok((context, wheels))
}

fn wait_for_file(path: &Path) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !path.exists() {
        ensure!(
            Instant::now() < deadline,
            "Timed out waiting for {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

#[test]
fn build_dependencies_preserve_external_lockfile_edits() -> Result<()> {
    let (context, wheels) = gated_build_lock_project()?;
    uv_snapshot!(context.filters(), context.lock()
        .args(["--offline", "--no-index", "--find-links"]).arg(wheels.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    let original = context.read("uv.lock");

    let child = context
        .lock()
        .args([
            "--build-dependencies",
            "--preview-features",
            "build-dependency-locking",
            "--offline",
            "--no-index",
            "--find-links",
        ])
        .arg(wheels.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    wait_for_file(&context.temp_dir.join("gate.ready"))?;
    let edited = format!("{original}\n# concurrent edit\n");
    context.temp_dir.child("uv.lock").write_str(&edited)?;
    context.temp_dir.child("gate.release").touch()?;
    let output = child.wait_with_output()?;
    assert!(!output.status.success());
    assert_snapshot!(apply_filters(String::from_utf8(output.stderr)?, context.filters()), @"
    Resolved 2 packages in [TIME]
    error: The lockfile changed while resolving; retry the command
    ");
    assert_eq!(context.read("uv.lock"), edited);
    Ok(())
}

#[test]
fn build_dependencies_interrupted_capture_preserves_lockfile() -> Result<()> {
    let (context, wheels) = gated_build_lock_project()?;
    uv_snapshot!(context.filters(), context.lock()
        .args(["--offline", "--no-index", "--find-links"]).arg(wheels.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    let original = context.read("uv.lock");

    let mut child = context
        .lock()
        .args([
            "--build-dependencies",
            "--preview-features",
            "build-dependency-locking",
            "--offline",
            "--no-index",
            "--find-links",
        ])
        .arg(wheels.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    wait_for_file(&context.temp_dir.join("gate.ready"))?;
    child.kill()?;
    context.temp_dir.child("gate.release").touch()?;
    assert!(!child.wait()?.success());
    assert_eq!(context.read("uv.lock"), original);

    uv_snapshot!(context.filters(), context.lock()
        .args(["--build-dependencies", "--preview-features", "build-dependency-locking",
            "--offline", "--no-index", "--find-links"])
        .arg(wheels.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    Ok(())
}

#[test]
fn build_dependencies_serialize_concurrent_writers() -> Result<()> {
    let (context, wheels) = gated_build_lock_project()?;
    uv_snapshot!(context.filters(), context.lock()
        .args(["--offline", "--no-index", "--find-links"]).arg(wheels.path()), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    ");
    let ordinary = context.read("uv.lock");

    let capture = context
        .lock()
        .args([
            "--build-dependencies",
            "--preview-features",
            "build-dependency-locking",
            "--offline",
            "--no-index",
            "--find-links",
        ])
        .arg(wheels.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    wait_for_file(&context.temp_dir.join("gate.ready"))?;
    let other = context.with_cache_dir("concurrent-cache");
    let mut removal = other
        .lock()
        .args([
            "--no-build-dependencies",
            "--offline",
            "--no-index",
            "--find-links",
        ])
        .arg(wheels.path())
        .arg("--verbose")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    let stderr = removal.stderr.take().expect("piped stderr");
    let (send, receive) = mpsc::channel();
    let reader = thread::spawn(move || -> Result<String> {
        let mut output = String::new();
        for line in BufReader::new(stderr).lines() {
            let line = line?;
            if line.contains("Waiting to acquire exclusive lock") && line.contains("uv-lockfile-") {
                let _ = send.send(());
            }
            output.push_str(&line);
            output.push('\n');
        }
        Ok(output)
    });
    receive.recv_timeout(Duration::from_secs(30))?;
    other.temp_dir.child("gate.release").touch()?;

    let output = capture.wait_with_output()?;
    assert!(output.status.success());
    assert_snapshot!(apply_filters(String::from_utf8(output.stderr)?, other.filters()), @"Resolved 2 packages in [TIME]");
    let status = removal.wait()?;
    let stderr = reader.join().expect("stderr reader")?;
    assert!(status.success());
    let other = other.with_filter((r"(?m)^DEBUG .*\n", "")).with_filter((
        r"at `[^`\r\n]*[/\\]uv-lockfile-[0-9a-f]+\.lock`",
        "at `[LOCKFILE_LOCK]`",
    ));
    assert_snapshot!(apply_filters(stderr, other.filters()), @"
    INFO Waiting to acquire exclusive lock for `[TEMP_DIR]/uv.lock` at `[LOCKFILE_LOCK]`
    Resolved 2 packages in [TIME]
    ");
    assert_eq!(other.read("uv.lock"), ordinary);
    Ok(())
}

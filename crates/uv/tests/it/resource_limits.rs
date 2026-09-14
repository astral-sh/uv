use anyhow::Result;
use assert_fs::fixture::{FileWriteStr, PathChild};
use indoc::formatdoc;
use uv_static::EnvVars;
use uv_test::{get_bin, uv_snapshot};

#[test]
fn adjust_open_file_limit() {
    let context = uv_test::test_context!("3.12");
    let python = &context.python_versions[0].1;

    let mut command = context.external_command("sh");
    command
        .arg("-c")
        .arg("ulimit -S -n 128; exec \"$@\"")
        .arg("sh")
        .arg(get_bin!())
        .arg("run")
        .arg("--no-project")
        .arg("--")
        .arg(python)
        .arg("-c")
        .arg("import resource; print(resource.getrlimit(resource.RLIMIT_NOFILE)[0] > 128)")
        .env(EnvVars::UV_CACHE_DIR, context.cache_dir.path());

    uv_snapshot!(context.filters(), command, @r"
    exit_code: 0 (success)
    ----- stdout -----
    True
    ");
}

#[test]
fn run_open_file_limit_override() {
    let context = uv_test::test_context!("3.12");
    let python = &context.python_versions[0].1;

    let mut command = context.run();
    command
        .arg("--no-project")
        .arg("--")
        .arg(python)
        .arg("-c")
        .arg(
            "import resource; soft, hard = resource.getrlimit(resource.RLIMIT_NOFILE); print(soft); print(hard > soft)",
        )
        .env(EnvVars::UV_RUN_RLIMIT_NOFILE, "128");

    uv_snapshot!(context.filters(), command, @r"
    exit_code: 0 (success)
    ----- stdout -----
    128
    True
    ");
}

#[test]
fn run_open_file_limit_override_invalid() {
    let context = uv_test::test_context!("3.12");
    let python = &context.python_versions[0].1;

    let mut command = context.run();
    command
        .arg("--no-project")
        .arg("--")
        .arg(python)
        .arg("-c")
        .arg("pass")
        .env(EnvVars::UV_RUN_RLIMIT_NOFILE, "invalid");

    uv_snapshot!(context.filters(), command, @r"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to parse environment variable `UV_RUN_RLIMIT_NOFILE` with invalid value `invalid`: invalid digit found in string
    ");
}

#[test]
fn run_open_file_limit_override_exceeds_hard_limit() {
    let context = uv_test::test_context!("3.12");
    let python = &context.python_versions[0].1;

    let mut command = context.external_command("sh");
    command
        .arg("-c")
        .arg("ulimit -S -n 128; ulimit -H -n 128; exec \"$@\"")
        .arg("sh")
        .arg(get_bin!())
        .arg("run")
        .arg("--no-project")
        .arg("--")
        .arg(python)
        .arg("-c")
        .arg("pass")
        .env(EnvVars::UV_CACHE_DIR, context.cache_dir.path())
        .env(EnvVars::UV_RUN_RLIMIT_NOFILE, "256");

    uv_snapshot!(context.filters(), command, @r"
    exit_code: 2 (failure)
    ----- stderr -----
    error: Failed to apply `UV_RUN_RLIMIT_NOFILE` value `256`
      cause: requested open file limit (256) exceeds the hard limit (128)
    ");
}

/// A minimal PEP 517 build backend for `sync_workspace_under_low_open_file_limit`.
///
/// It builds an editable wheel containing a single `.pth` file, so building a
/// workspace member requires no network access and no compilation, while still
/// spawning a build backend process (and its pipes) for every member.
const TEST_BACKEND: &str = r#"import base64
import csv
import hashlib
import io
import tomllib
import zipfile
from pathlib import Path


def get_requires_for_build_editable(config_settings=None):
    return []


def build_editable(wheel_directory, config_settings=None, metadata_directory=None):
    data = tomllib.loads(Path("pyproject.toml").read_text())
    project = data["project"]
    name = project["name"]
    version = project["version"]

    dist_info = f"{name.replace('-', '_')}-{version}.dist-info"
    record = io.StringIO()
    csv.writer(record, lineterminator="\n").writerows(
        [("WHEEL", "", ""), ("METADATA", "", "")]
    )
    contents = {
        f"{dist_info}/WHEEL": (
            "Wheel-Version: 1.0\nGenerator: test\nRoot-Is-Purelib: true\nTag: py3-none-any\n"
        ),
        f"{dist_info}/METADATA": (
            f"Metadata-Version: 2.1\nName: {name}\nVersion: {version}\n"
            f"Requires-Python: {project.get('requires-python', '>=3.8')}\n"
        ),
        f"{name.replace('-', '_')}.pth": ".\n",
    }
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as archive:
        for path, data in contents.items():
            archive.writestr(path, data)
            digest = base64.urlsafe_b64encode(hashlib.sha256(data.encode()).digest())
            checksum = digest.rstrip(b"=").decode()
            csv.writer(record, lineterminator="\n").writerow(
                (path, f"sha256={checksum}", "")
            )
        archive.writestr(f"{dist_info}/RECORD", record.getvalue())

    filename = f"{name.replace('-', '_')}-{version}-py3-none-any.whl"
    Path(wheel_directory).joinpath(filename).write_bytes(buf.getvalue())
    return filename
"#;

/// Syncing a large workspace must not exhaust the process's open file limit.
///
/// Every concurrent build holds several file descriptors (the build backend
/// process, its pipes, and the files written to the cache), so with more
/// concurrent builds than the open file limit allows, `uv sync` used to fail
/// with `Too many open files (os error 24)`. Here, both the soft and the hard
/// limit are set low, so uv cannot raise its soft limit to make room.
///
/// See: <https://github.com/astral-sh/uv/issues/11296>
#[test]
fn sync_workspace_under_low_open_file_limit() -> Result<()> {
    let context = uv_test::test_context!("3.12");

    let members = 20;
    let dependencies = (1..=members)
        .map(|member| format!("\"pkg-{member}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let sources = (1..=members)
        .map(|member| format!("pkg-{member} = {{ workspace = true }}"))
        .collect::<Vec<_>>()
        .join("\n");

    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&formatdoc! {r#"
            [project]
            name = "root"
            version = "0.1.0"
            requires-python = ">=3.12"
            dependencies = [{dependencies}]

            [tool.uv.sources]
            {sources}

            [tool.uv.workspace]
            members = ["pkg-*"]
        "#})?;

    for member in 1..=members {
        let member_dir = context.temp_dir.child(format!("pkg-{member}"));
        member_dir
            .child("pyproject.toml")
            .write_str(&formatdoc! {r#"
            [project]
            name = "pkg-{member}"
            version = "0.1.0"
            requires-python = ">=3.12"
            dependencies = []

            [build-system]
            requires = []
            build-backend = "backend"
            backend-path = ["."]
            "#})?;
        member_dir.child("backend.py").write_str(TEST_BACKEND)?;
    }

    // Run `uv sync` with an open file limit low enough that syncing all members
    // concurrently fails, while leaving enough room for a capped run to succeed.
    // The concurrency limits are raised explicitly, so that the number of
    // concurrent builds doesn't depend on the machine's CPU count.
    let mut command = context.external_command("sh");
    command
        .arg("-c")
        .arg("ulimit -n 64; exec \"$@\"")
        .arg("sh")
        .arg(get_bin!())
        .arg("sync")
        .arg("--offline")
        .env(EnvVars::UV_CACHE_DIR, context.cache_dir.path())
        .env(EnvVars::UV_CONCURRENT_BUILDS, "32")
        .env(EnvVars::UV_CONCURRENT_INSTALLS, "32");

    uv_snapshot!(context.filters(), command, @r"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 21 packages in [TIME]
    Prepared 20 packages in [TIME]
    Installed 20 packages in [TIME]
     + pkg-1==0.1.0 (from file://[TEMP_DIR]/pkg-1)
     + pkg-10==0.1.0 (from file://[TEMP_DIR]/pkg-10)
     + pkg-11==0.1.0 (from file://[TEMP_DIR]/pkg-11)
     + pkg-12==0.1.0 (from file://[TEMP_DIR]/pkg-12)
     + pkg-13==0.1.0 (from file://[TEMP_DIR]/pkg-13)
     + pkg-14==0.1.0 (from file://[TEMP_DIR]/pkg-14)
     + pkg-15==0.1.0 (from file://[TEMP_DIR]/pkg-15)
     + pkg-16==0.1.0 (from file://[TEMP_DIR]/pkg-16)
     + pkg-17==0.1.0 (from file://[TEMP_DIR]/pkg-17)
     + pkg-18==0.1.0 (from file://[TEMP_DIR]/pkg-18)
     + pkg-19==0.1.0 (from file://[TEMP_DIR]/pkg-19)
     + pkg-2==0.1.0 (from file://[TEMP_DIR]/pkg-2)
     + pkg-20==0.1.0 (from file://[TEMP_DIR]/pkg-20)
     + pkg-3==0.1.0 (from file://[TEMP_DIR]/pkg-3)
     + pkg-4==0.1.0 (from file://[TEMP_DIR]/pkg-4)
     + pkg-5==0.1.0 (from file://[TEMP_DIR]/pkg-5)
     + pkg-6==0.1.0 (from file://[TEMP_DIR]/pkg-6)
     + pkg-7==0.1.0 (from file://[TEMP_DIR]/pkg-7)
     + pkg-8==0.1.0 (from file://[TEMP_DIR]/pkg-8)
     + pkg-9==0.1.0 (from file://[TEMP_DIR]/pkg-9)
    ");

    Ok(())
}

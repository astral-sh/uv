use std::io::Write;
use std::path::Path;

use anyhow::Result;
use assert_fs::fixture::{FileWriteStr, PathChild, PathCreateDir};
use fs_err::File;
use indoc::indoc;
use url::Url;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use uv_test::uv_snapshot;

fn write_wheel(
    path: &Path,
    name: &str,
    dist_info_prefix: &str,
    files: &[(&str, &str)],
) -> Result<()> {
    let mut writer = ZipWriter::new(File::create(path)?);
    let options = SimpleFileOptions::default();
    let mut record = Vec::new();

    for (file_path, contents) in files {
        writer.start_file(file_path, options)?;
        writer.write_all(contents.as_bytes())?;
        record.push(format!("{file_path},,"));
    }

    let metadata_path = format!("{dist_info_prefix}.dist-info/METADATA");
    writer.start_file(&metadata_path, options)?;
    writer
        .write_all(format!("Metadata-Version: 2.1\nName: {name}\nVersion: 0.1.0\n").as_bytes())?;
    record.push(format!("{metadata_path},,"));

    let wheel_path = format!("{dist_info_prefix}.dist-info/WHEEL");
    writer.start_file(&wheel_path, options)?;
    writer.write_all(
        b"Wheel-Version: 1.0\nGenerator: uv-test\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
    )?;
    record.push(format!("{wheel_path},,"));

    let record_path = format!("{dist_info_prefix}.dist-info/RECORD");
    record.push(format!("{record_path},,"));
    writer.start_file(&record_path, options)?;
    writer.write_all(record.join("\n").as_bytes())?;
    writer.write_all(b"\n")?;

    writer.finish()?;
    Ok(())
}

fn write_ty_wrapper(context: &uv_test::TestContext) -> Result<()> {
    let script = context.bin_dir.child("ty.py");
    script.write_str(indoc! {r#"
        import json
        import os
        import pathlib
        import sys

        args = sys.argv[1:]
        assert args and args[0] == "check", args
        metadata_path = pathlib.Path(args[args.index("--dependency-metadata") + 1])
        data = json.loads(metadata_path.read_text())

        expected_target = os.environ.get("UV_TEST_EXPECTED_TY_TARGET")
        if expected_target:
            target = pathlib.Path(args[-1])
            pyproject = target / "pyproject.toml"
            assert pyproject.is_file(), args
            assert f'name = "{expected_target}"' in pyproject.read_text(), pyproject.read_text()

        if os.environ.get("UV_TEST_EXPECT_MODULE_OWNERS"):
            module_owners = data["module_owners"]
            assert list(module_owners) == ["typing_extensions"], module_owners
            owners = module_owners["typing_extensions"]
            assert len(owners) == 1, owners
            assert owners[0].startswith("typing-extensions==0.1.0@path+"), owners
            assert owners[0].endswith("/typing_extensions-0.1.0-py3-none-any.whl"), owners

        print("ty ok")
        print("members=" + ",".join(member["name"] for member in data["members"]))
        if expected_target:
            print(f"target={expected_target}")
        if "module_owners" in data:
            print("module_owners=" + ",".join(sorted(data["module_owners"])))
    "#})?;

    #[cfg(windows)]
    {
        let wrapper = context.bin_dir.child("ty.cmd");
        let python = &context.python_versions[0].1;
        wrapper.write_str(&format!(
            "@echo off\r\n\"{}\" \"%~dp0ty.py\" %*\r\n",
            python.display()
        ))?;
    }

    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;

        let wrapper = context.bin_dir.child("ty");
        let python = &context.python_versions[0].1;
        wrapper.write_str(&format!(
            "#!{}\nimport pathlib\nimport runpy\nrunpy.run_path(str(pathlib.Path(__file__).with_name('ty.py')), run_name='__main__')\n",
            python.display()
        ))?;

        let mut permissions = fs_err::metadata(wrapper.path())?.permissions();
        permissions.set_mode(0o755);
        fs_err::set_permissions(wrapper.path(), permissions)?;
    }

    Ok(())
}

#[test]
fn mow_passes_workspace_metadata_to_ty() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_counts();

    write_ty_wrapper(&context)?;

    let typing_extensions = context
        .temp_dir
        .child("typing_extensions-0.1.0-py3-none-any.whl");
    write_wheel(
        typing_extensions.path(),
        "typing-extensions",
        "typing_extensions-0.1.0",
        &[("typing_extensions.py", "")],
    )?;
    let typing_extensions_url = Url::from_file_path(typing_extensions.path())
        .map_err(|()| anyhow::anyhow!("failed to convert wheel path to file URL"))?;

    context
        .temp_dir
        .child("src")
        .child("mow_project")
        .create_dir_all()?;
    context
        .temp_dir
        .child("src")
        .child("mow_project")
        .child("__init__.py")
        .write_str("")?;
    context
        .temp_dir
        .child("pyproject.toml")
        .write_str(&format!(
            r#"[project]
name = "mow-project"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = [
  "typing-extensions @ {typing_extensions_url}",
]
"#
        ))?;

    uv_snapshot!(context.filters(), context.mow()
        .env("UV_TEST_EXPECTED_TY_TARGET", "mow-project")
        .env("UV_TEST_EXPECT_MODULE_OWNERS", "1"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    ty ok
    members=mow-project
    target=mow-project
    module_owners=typing_extensions

    ----- stderr -----
    warning: `uv mow` is experimental and may change without warning. Pass `--preview-features mow` to disable this warning.
    Resolved [N] packages in [TIME]
    "#);

    Ok(())
}

#[test]
fn mow_checks_workspace_member_target() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filtered_counts();

    write_ty_wrapper(&context)?;

    context.temp_dir.child("pyproject.toml").write_str(
        r#"[project]
name = "root"
version = "0.1.0"
requires-python = ">=3.12"

[tool.uv]
package = false

[tool.uv.workspace]
members = ["member"]
"#,
    )?;

    let member = context.temp_dir.child("member");
    member.create_dir_all()?;
    member.child("pyproject.toml").write_str(
        r#"[project]
name = "member"
version = "0.1.0"
requires-python = ">=3.12"

[tool.uv]
package = false
"#,
    )?;

    uv_snapshot!(context.filters(), context.mow()
        .current_dir(member.path())
        .env("UV_TEST_EXPECTED_TY_TARGET", "member"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    ty ok
    members=member,root
    target=member

    ----- stderr -----
    warning: `uv mow` is experimental and may change without warning. Pass `--preview-features mow` to disable this warning.
    Resolved [N] packages in [TIME]
    "#);

    uv_snapshot!(context.filters(), context.mow()
        .arg("--package")
        .arg("member")
        .env("UV_TEST_EXPECTED_TY_TARGET", "member"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    ty ok
    members=member,root
    target=member

    ----- stderr -----
    warning: `uv mow` is experimental and may change without warning. Pass `--preview-features mow` to disable this warning.
    Resolved [N] packages in [TIME]
    "#);

    Ok(())
}

//! PEP 825 metadata in interoperable lockfiles.

use anyhow::Result;
use assert_fs::prelude::*;
use indoc::indoc;
use serde_json::json;

use uv_test::uv_snapshot;

use super::variants::{write_metadata, write_wheel};

#[test]
fn pep825_pylock_metadata() -> Result<()> {
    let context = uv_test::test_context!("3.12").with_filter((r"[a-f0-9]{64}", "[HASH]"));
    let properties = json!({"gpu": {"cuda": ["12.0", "13.0", "14.0"]}});
    write_wheel(
        &context,
        "example",
        Some("fast"),
        properties.clone(),
        &[],
        None,
    )?;
    write_wheel(&context, "example", Some("null"), json!({}), &[], None)?;
    write_wheel(&context, "example", None, json!({}), &[], None)?;
    write_metadata(&context, json!({"fast": properties, "null": {}}))?;
    context
        .temp_dir
        .child("requirements.in")
        .write_str("example")?;
    uv_snapshot!(context.filters(), context.pip_compile().arg("--preview-features").arg("wheel-variants")
        .arg("requirements.in").arg("--no-index").arg("--find-links").arg(context.temp_dir.path())
        .arg("--universal").arg("--no-header").arg("-o").arg("pylock.toml"), @r###"
    exit_code: 0 (success)
    ----- stdout -----
    lock-version = "1.0"
    created-by = "uv"
    requires-python = ">=3.12"

    [[packages]]
    name = "example"
    version = "1.0.0"
    wheels = [
        { url = "file://[TEMP_DIR]/example-1.0.0-py3-none-any.whl", hashes = { sha256 = "[HASH]" } },
        { url = "file://[TEMP_DIR]/example-1.0.0-py3-none-any-fast.whl", hashes = { sha256 = "[HASH]" } },
        { url = "file://[TEMP_DIR]/example-1.0.0-py3-none-any-null.whl", hashes = { sha256 = "[HASH]" } },
    ]

    [packages.variants-json]
    "$schema" = "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json"
    default-priorities = { namespace = ["gpu", "cpu"] }
    variants = { fast = { gpu = { cuda = ["12.0", "13.0", "14.0"] } }, null = {} }

    ----- stderr -----
    Resolved 1 package in [TIME]
    "###);
    // The exported file must remain usable when the mutable index metadata changes.
    context
        .temp_dir
        .child("example-1.0.0-variants.json")
        .write_str("{}")?;
    // The separate pylock preview does not enable variants or read the target property file.
    uv_snapshot!(context.filters(), context.pip_sync()
        .arg("--preview-features").arg("pylock").arg("pylock.toml")
        .env("UV_VARIANT_LOCK", context.temp_dir.child("missing.toml").path()), @r###"
    exit_code: 2 (failure)
    ----- stderr -----
    error: This lockfile uses wheel variants; pass `--preview-features wheel-variants` to use it
    "###);
    uv_snapshot!(context.filters(), context.pip_sync().arg("--preview-features").arg("pylock,wheel-variants").arg("pylock.toml")
        .env("UV_VARIANT_LOCK", context.temp_dir.child("target.toml").path()), @r###"
    exit_code: 0 (success)
    ----- stderr -----
    Prepared 1 package in [TIME]
    Installed 1 package in [TIME]
     + example==1.0.0
    "###);
    uv_snapshot!(context.filters(), context.python_command().arg("-c").arg(indoc! {r#"
        import example
        from pathlib import Path
        print(Path(example.__file__).with_name("selected.txt").read_text())
    "#}), @r###"
    exit_code: 0 (success)
    ----- stdout -----
    fast build 0
    "###);
    Ok(())
}

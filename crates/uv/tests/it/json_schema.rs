use std::path::Path;

use assert_cmd::prelude::*;
use indoc::indoc;
use uv_test::uv_snapshot;

#[test]
fn preview_options_fastjsonschema() {
    let context = uv_test::test_context!("3.12").with_exclude_newer("2025-08-15T00:00:00Z");
    let schema = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../uv.schema.json");

    context
        .pip_install()
        .arg("fastjsonschema==2.21.2")
        .assert()
        .success();

    uv_snapshot!(context.filters(), context.python_command()
        .arg("-c")
        .arg(indoc! {r#"
            import json
            import sys
            from pathlib import Path

            import fastjsonschema

            schema = json.loads(Path(sys.argv[1]).read_text())
            validate = fastjsonschema.compile(schema, use_formats=False)

            validate({})
            validate({"preview": True})
            validate({"preview-features": False})

            try:
                validate({"preview": True, "preview-features": False})
            except fastjsonschema.JsonSchemaValueException:
                pass
            else:
                raise AssertionError("both preview settings were accepted")

            print("Preview schema validated")
        "#})
        .arg(schema), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Preview schema validated

    ----- stderr -----
    ");
}

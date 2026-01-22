use uv_test::uv_snapshot;

#[test]
fn debug_warn() {
    let context = uv_test::test_context!("3.12");

    uv_snapshot!(context.pip_debug(), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: pip's `debug` is unsupported (consider using `uvx pip debug` instead)
    "
    );
}

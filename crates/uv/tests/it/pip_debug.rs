use crate::common::{TestContext, uv_snapshot};

#[test]
fn debug_warn() {
    let context = TestContext::new("3.12");

    uv_snapshot!(context.pip_debug(), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: pip's `debug` is unsupported (consider using `uvx pip debug` instead)
    "
    );
}

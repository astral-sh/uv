//! Wheel variant scenarios ported from <https://github.com/astral-sh/packse/pull/292>.

use std::process::Command;

use indoc::indoc;

use uv_test::packse::PackseServer;
use uv_test::{TestContext, uv_snapshot};

fn command(context: &TestContext, server: &PackseServer, cpu_level: u8) -> Command {
    let mut command = context.pip_install();
    command
        .arg("--index-url")
        .arg(server.index_url())
        .env("PROVIDER_CPU_LEVEL", cpu_level.to_string())
        .env_remove("UV_VARIANT_LOCK")
        .env_remove("UV_VARIANT_LOCK_INCOMPLETE");
    command
}

/// Show the complete environment, including the selected labels from installed wheel metadata.
/// This also checks that the runtime provider stays in its isolated environment.
fn installed_variants(context: &TestContext) -> Command {
    let mut command = context.python_command();
    command.arg("-c").arg(indoc! {r#"
        import json
        from importlib.metadata import distributions

        for dist in sorted(distributions(), key=lambda dist: dist.metadata["Name"]):
            metadata = dist.read_text("variant.json")
            label = next(iter(json.loads(metadata)["variants"])) if metadata else "non-variant"
            print(f"{dist.metadata['Name']}=={dist.version} ({label})")
    "#});
    command
}

/// Namespace order determines whether CPU level or BLAS library wins. The library's build-time
/// provider must not be installed; only the mock CPU provider runs at installation time.
#[test]
fn variants_basic() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("variants/variants-basic.toml");

    uv_snapshot!(context.filters(), command(&context, &server, 3)
        .arg("cpu-first").arg("blas-first"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + blas-first==1.0.0
     + cpu-first==1.0.0
    ");
    uv_snapshot!(context.filters(), installed_variants(&context), @"
    exit_code: 0 (success)
    ----- stdout -----
    blas-first==1.0.0 (openblas_v2)
    cpu-first==1.0.0 (closedblas_v3)
    ");

    // Query the provider again with a lower CPU level, even with the wheels already cached.
    uv_snapshot!(context.filters(), command(&context, &server, 2)
        .arg("cpu-first").arg("blas-first").arg("--reinstall"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Uninstalled 2 packages in [TIME]
    Installed 2 packages in [TIME]
     ~ blas-first==1.0.0
     ~ cpu-first==1.0.0
    ");
    uv_snapshot!(context.filters(), installed_variants(&context), @"
    exit_code: 0 (success)
    ----- stdout -----
    blas-first==1.0.0 (openblas_v2)
    cpu-first==1.0.0 (openblas_v2)
    ");
}

/// Backtrack past a version with no compatible wheels, and distinguish null from non-variant
/// fallback wheels.
#[test]
fn variants_fallback() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("variants/variants-fallback.toml");

    uv_snapshot!(context.filters(), command(&context, &server, 3)
        .arg("only-fallback").arg("null-variant"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 2 packages in [TIME]
    Prepared 2 packages in [TIME]
    Installed 2 packages in [TIME]
     + null-variant==1.0.0
     + only-fallback==1.0.0
    ");
    uv_snapshot!(context.filters(), installed_variants(&context), @"
    exit_code: 0 (success)
    ----- stdout -----
    null-variant==1.0.0 (null)
    only-fallback==1.0.0 (non-variant)
    ");
}

/// Markers see every supported property of the selected wheel, including values below the
/// preferred CPU level, and ignore properties from other wheels.
#[test]
fn variants_marker() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("variants/variants-marker.toml");

    uv_snapshot!(context.filters(), command(&context, &server, 3).arg("a"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 5 packages in [TIME]
    Prepared 5 packages in [TIME]
    Installed 5 packages in [TIME]
     + a==1.0.0
     + cpu-v1==1.0.0
     + cpu-v3==1.0.0
     + feature-cpu-level==1.0.0
     + namespace-cpu==1.0.0
    ");
    uv_snapshot!(context.filters(), installed_variants(&context), @"
    exit_code: 0 (success)
    ----- stdout -----
    a==1.0.0 (selected)
    cpu-v1==1.0.0 (non-variant)
    cpu-v3==1.0.0 (non-variant)
    feature-cpu-level==1.0.0 (non-variant)
    namespace-cpu==1.0.0 (non-variant)
    ");
}

/// The same wheel is compatible with a v1 CPU, but its v3 dependency must not be installed.
#[test]
fn variants_marker_supported_properties() {
    let context = uv_test::test_context!("3.12");
    let server = PackseServer::new("variants/variants-marker.toml");

    uv_snapshot!(context.filters(), command(&context, &server, 1).arg("a"), @"
    exit_code: 0 (success)
    ----- stderr -----
    Resolved 4 packages in [TIME]
    Prepared 4 packages in [TIME]
    Installed 4 packages in [TIME]
     + a==1.0.0
     + cpu-v1==1.0.0
     + feature-cpu-level==1.0.0
     + namespace-cpu==1.0.0
    ");
    uv_snapshot!(context.filters(), installed_variants(&context), @"
    exit_code: 0 (success)
    ----- stdout -----
    a==1.0.0 (selected)
    cpu-v1==1.0.0 (non-variant)
    feature-cpu-level==1.0.0 (non-variant)
    namespace-cpu==1.0.0 (non-variant)
    ");
}

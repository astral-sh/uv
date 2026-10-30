//! Reproduction of the invalid-release boundary in Darwin version-aware markers.
use uv_pep508::{MarkerEnvironment, MarkerEnvironmentBuilder, MarkerTree};

#[test]
fn darwin_unparseable_release_negation() {
    // Version | String fields fall back to opaque string semantics on parse
    // failure, making this equality false and its negation true.
    // https://packaging.python.org/en/latest/specifications/dependency-specifiers/#marker-comparisons
    let env = MarkerEnvironment::try_from(MarkerEnvironmentBuilder {
        implementation_name: "cpython",
        implementation_version: "3.12.0",
        os_name: "posix",
        platform_machine: "aarch64",
        platform_python_implementation: "CPython",
        platform_release: "not-a-version",
        platform_system: "Darwin",
        platform_version: "",
        python_full_version: "3.12.0",
        python_version: "3.12",
        sys_platform: "darwin",
    })
    .expect("only Version fields require valid versions");
    let marker: MarkerTree = "platform_release == '24'"
        .parse()
        .expect("release marker parses");
    assert!(!marker.evaluate(&env, &[]));
    assert!(marker.negate().evaluate(&env, &[]));
}

#[test]
fn invalid_darwin_release_is_not_a_numeric_tautology() {
    let env = MarkerEnvironment::try_from(MarkerEnvironmentBuilder {
        implementation_name: "cpython",
        implementation_version: "3.12",
        os_name: "posix",
        platform_machine: "aarch64",
        platform_python_implementation: "CPython",
        platform_release: "3-invalid",
        platform_system: "Darwin",
        platform_version: "",
        python_full_version: "3.12",
        python_version: "3.12",
        sys_platform: "darwin",
    })
    .unwrap();
    let marker: MarkerTree = "platform_release >= '9' or platform_release < '24'"
        .parse()
        .unwrap();
    assert!(!marker.evaluate(&env, &[]));
}

use std::error::Error;

use uv_preview::{PreviewFeature, test::with_features};

use super::{MarkerEnvironment, MarkerEnvironmentBuilder, MarkerTree};

#[test]
fn sys_abi_features_evaluation() -> Result<(), Box<dyn Error>> {
    let _preview = with_features(&[PreviewFeature::SysAbiFeatures]);
    let env = MarkerEnvironment::try_from(MarkerEnvironmentBuilder {
        implementation_name: "cpython",
        implementation_version: "3.13.0",
        os_name: "posix",
        platform_machine: "x86_64",
        platform_python_implementation: "CPython",
        platform_release: "",
        platform_system: "Linux",
        platform_version: "",
        python_full_version: "3.13.0",
        python_version: "3.13",
        sys_platform: "linux",
    })?
    .with_sys_abi_features(["free-threading", "64-bit"].map(str::to_owned));
    for (feature, expected) in [
        ("free-threading", true),
        ("gil-enabled", false),
        ("64-bit", true),
        ("32-bit", false),
        ("debug", false),
        ("Free-Threading", false),
        ("future-feature", false),
    ] {
        let marker = format!("'{feature}' in sys_abi_features").parse::<MarkerTree>()?;
        assert_eq!(marker.evaluate(&env, &[]), expected, "{feature}");
        assert_eq!(marker.negate().evaluate(&env, &[]), !expected, "{feature}");
        assert_eq!(
            marker
                .contents()
                .expect("nontrivial marker")
                .to_string()
                .parse::<MarkerTree>()?,
            marker
        );
    }
    let serialized = serde_json::to_string(&env)?;
    let restored: MarkerEnvironment = serde_json::from_str(&serialized)?;
    assert_eq!(restored.sys_abi_features(), env.sys_abi_features());
    for invalid in [
        "sys_abi_features in 'debug'",
        "'debug' == sys_abi_features",
        "'debug' >= sys_abi_features",
    ] {
        assert!(invalid.parse::<MarkerTree>().is_err(), "{invalid}");
    }
    Ok(())
}

#[test]
fn sys_abi_features_conflicts() -> Result<(), Box<dyn Error>> {
    let _preview = with_features(&[PreviewFeature::SysAbiFeatures]);
    for impossible in [
        "'free-threading' in sys_abi_features and 'gil-enabled' in sys_abi_features",
        "'32-bit' in sys_abi_features and '64-bit' in sys_abi_features",
        "'debug' in sys_abi_features and platform_python_implementation == 'PyPy'",
        "'free-threading' not in sys_abi_features and 'gil-enabled' not in sys_abi_features and platform_python_implementation == 'CPython'",
    ] {
        assert!(impossible.parse::<MarkerTree>()?.is_false(), "{impossible}");
    }
    let regular: MarkerTree =
        "'free-threading' not in sys_abi_features and platform_python_implementation == 'CPython'"
            .parse()?;
    let gil: MarkerTree = "'gil-enabled' in sys_abi_features".parse()?;
    assert!(regular.is_disjoint(gil.negate()));
    assert!(gil.is_disjoint(regular.negate()));
    Ok(())
}

#[test]
fn sys_abi_features_requires_preview() {
    let _preview = with_features(&[]);
    let error = "'free-threading' in sys_abi_features"
        .parse::<MarkerTree>()
        .expect_err("preview disabled");
    assert!(
        error
            .to_string()
            .contains("--preview-features sys-abi-features")
    );
}

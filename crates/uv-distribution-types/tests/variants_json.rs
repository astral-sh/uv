use insta::{assert_json_snapshot, assert_snapshot};
use serde_json::json;

use uv_distribution_types::{
    PEP_825_SCHEMA, PEP_850_SCHEMA, SupportedVariantProperties, VariantsJsonContent,
};

/// PEP 825 compares the best compatible value for each feature, then the variant label.
#[test]
fn rank_variants() -> Result<(), Box<dyn std::error::Error>> {
    let metadata: VariantsJsonContent = serde_json::from_value(json!({
        "$schema": PEP_825_SCHEMA,
        "default-priorities": {"namespace": ["nvidia", "x86_64", "other"]},
        "variants": {
            "gpu": {"nvidia": {"cuda": ["13.0"]}},
            "gpu_cpuv2": {"nvidia": {"cuda": ["13.0"]}, "x86_64": {"level": ["v2"]}},
            "gpu_cpuv4": {"nvidia": {"cuda": ["13.0"]}, "x86_64": {"level": ["v4"]}},
            "cpuv4": {"x86_64": {"level": ["v4"]}},
            "multiple_values": {"nvidia": {"cuda": ["12.0", "13.0", "14.0"]}},
            "older_cuda": {"nvidia": {"cuda": ["12.0"]}},
            "lower_feature": {"nvidia": {"arch": ["sm120"]}},
            "unsupported_value": {"nvidia": {"cuda": ["14.0"]}},
            "unsupported_feature": {"nvidia": {"other": ["on"]}},
            "unsupported_namespace": {"other": {"level": ["on"]}},
            "null": {}
        }
    }))?;
    let preferences: SupportedVariantProperties = serde_json::from_value(json!({
        "nvidia": [["cuda", ["13.0", "12.0"]], ["arch", ["sm120"]]],
        "x86_64": [["level", ["v4", "v3", "v2"]]]
    }))?;
    let mut ranked = [
        "gpu",
        "gpu_cpuv2",
        "gpu_cpuv4",
        "cpuv4",
        "multiple_values",
        "older_cuda",
        "lower_feature",
        "null",
        "unsupported_value",
        "unsupported_feature",
        "unsupported_namespace",
        "missing",
    ]
    .into_iter()
    .map(|label| Ok((label, metadata.score_variant(&label.parse()?, &preferences))))
    .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    ranked.sort_by(|left, right| right.1.cmp(&left.1));
    assert_snapshot!(
        ranked
            .iter()
            .map(|(label, score)| format!("{label}: {}", score.is_some()))
            .collect::<Vec<_>>()
            .join("\n"),
        @r#"
    gpu_cpuv4: true
    gpu_cpuv2: true
    gpu: true
    multiple_values: true
    older_cuda: true
    lower_feature: true
    cpuv4: true
    null: true
    unsupported_value: false
    unsupported_feature: false
    unsupported_namespace: false
    missing: false
    "#);
    Ok(())
}

#[test]
fn parse_static_provider() -> Result<(), Box<dyn std::error::Error>> {
    let metadata = json!({
        "$schema": PEP_850_SCHEMA,
        "default-priorities": {"namespace": ["cpu"]},
        "providers": {
            "cpu": {
                "optional": false,
                "static-properties": {"level": ["v3"], "flags": ["avx"]},
                "feature-order": ["level", "flags"]
            }
        },
        "variants": {"null": {}}
    });
    let parsed = serde_json::from_value::<VariantsJsonContent>(metadata.clone())?;
    assert_eq!(serde_json::to_value(parsed)?, metadata);
    Ok(())
}

#[test]
fn reject_missing_providers() {
    let invalid = json!({
        "$schema": PEP_850_SCHEMA,
        "default-priorities": {"namespace": ["cpu"]},
        "variants": {"null": {}}
    });
    let error =
        serde_json::from_value::<VariantsJsonContent>(invalid).expect_err("providers are required");
    assert_snapshot!(
        error,
        @r#"Variant providers must match the namespaces in default-priorities.namespace: expected ["cpu"], found []"#
    );
}

#[test]
fn reject_mismatched_provider_namespaces() {
    let invalid = json!({
        "$schema": PEP_850_SCHEMA,
        "default-priorities": {"namespace": ["gpu"]},
        "providers": {"cpu": {"static-properties": {"level": ["v3"]}}},
        "variants": {"null": {}}
    });
    let error = serde_json::from_value::<VariantsJsonContent>(invalid)
        .expect_err("providers must match the namespace priorities");
    assert_snapshot!(
        error,
        @r#"Variant providers must match the namespaces in default-priorities.namespace: expected ["gpu"], found ["cpu"]"#
    );
}

#[test]
fn reject_missing_static_feature_order() {
    let invalid = json!({
        "$schema": PEP_850_SCHEMA,
        "default-priorities": {"namespace": ["cpu"]},
        "providers": {
            "cpu": {
                "static-properties": {
                    "level": ["v3"],
                    "flags": ["avx"]
                }
            }
        },
        "variants": {"null": {}}
    });
    let error = serde_json::from_value::<VariantsJsonContent>(invalid)
        .expect_err("multiple features require an explicit order");
    assert_snapshot!(
        error,
        @r#"Variant provider `cpu` must define feature-order for multiple static features: ["flags", "level"]"#
    );
}

#[test]
fn reject_invalid_static_feature_order() {
    let invalid = json!({
        "$schema": PEP_850_SCHEMA,
        "default-priorities": {"namespace": ["cpu"]},
        "providers": {
            "cpu": {
                "static-properties": {"level": ["v3"]},
                "feature-order": ["other"]
            }
        },
        "variants": {"null": {}}
    });
    let error = serde_json::from_value::<VariantsJsonContent>(invalid)
        .expect_err("feature-order must match the static features");
    assert_snapshot!(
        error,
        @r#"Variant provider `cpu` has an invalid feature-order: expected a non-empty list containing each static feature in ["level"] exactly once, found ["other"]"#
    );
}

#[test]
fn reject_duplicate_static_property_values() {
    let invalid = json!({
        "$schema": PEP_850_SCHEMA,
        "default-priorities": {"namespace": ["cpu"]},
        "providers": {"cpu": {"static-properties": {"level": ["v3", "v3"]}}},
        "variants": {"null": {}}
    });
    let error = serde_json::from_value::<VariantsJsonContent>(invalid)
        .expect_err("static property values must be unique");
    assert_snapshot!(
        error,
        @r#"Variant feature `cpu :: level` must not contain duplicate values: ["v3", "v3"]"#
    );
}

/// Empty namespaces are removed during validation.
#[test]
fn normalize_variants_json() -> Result<(), Box<dyn std::error::Error>> {
    let metadata = json!({
        "$schema": PEP_825_SCHEMA,
        "default-priorities": {"namespace": ["nvidia", "x86_64"]},
        "variants": {
            "gpu": {"nvidia": {"cuda_version_lower_bound": ["13.0", "12.8"]}, "unused": {}},
            "cpu": {"x86_64": {"level": ["v3"]}},
            "null": {"unused": {}}
        }
    });
    let parsed = VariantsJsonContent::from_slice(&serde_json::to_vec(&metadata)?)?;

    assert_json_snapshot!(parsed, @r#"
    {
      "$schema": "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json",
      "default-priorities": {
        "namespace": [
          "nvidia",
          "x86_64"
        ]
      },
      "variants": {
        "cpu": {
          "x86_64": {
            "level": [
              "v3"
            ]
          }
        },
        "gpu": {
          "nvidia": {
            "cuda_version_lower_bound": [
              "13.0",
              "12.8"
            ]
          }
        },
        "null": {}
      }
    }
    "#);
    Ok(())
}

#[test]
fn reject_empty_namespace_priorities() {
    let invalid = json!({
        "$schema": PEP_825_SCHEMA,
        "default-priorities": {"namespace": []},
        "variants": {"fast": {"cpu": {"level": ["v2", "v3"]}}, "null": {}}
    });
    let error = serde_json::from_value::<VariantsJsonContent>(invalid)
        .expect_err("namespace priorities must not be empty");
    assert_snapshot!(error, @"Variant namespace priorities must be non-empty and unique");
}

#[test]
fn reject_duplicate_namespace_priorities() {
    let invalid = json!({
        "$schema": PEP_825_SCHEMA,
        "default-priorities": {"namespace": ["cpu", "cpu"]},
        "variants": {"fast": {"cpu": {"level": ["v2", "v3"]}}, "null": {}}
    });
    let error = serde_json::from_value::<VariantsJsonContent>(invalid)
        .expect_err("namespace priorities must be unique");
    assert_snapshot!(error, @"Variant namespace priorities must be non-empty and unique");
}

#[test]
fn reject_invalid_namespace_in_priorities() {
    let invalid = json!({
        "$schema": PEP_825_SCHEMA,
        "default-priorities": {"namespace": ["CPU"]},
        "variants": {"fast": {"cpu": {"level": ["v2", "v3"]}}, "null": {}}
    });
    let error = serde_json::from_value::<VariantsJsonContent>(invalid)
        .expect_err("namespace priorities must be valid");
    assert_snapshot!(error, @"Invalid variant namespace: `CPU`");
}

#[test]
fn reject_namespace_missing_from_priorities() {
    let invalid = json!({
        "$schema": PEP_825_SCHEMA,
        "default-priorities": {"namespace": ["gpu"]},
        "variants": {"fast": {"cpu": {"level": ["v2", "v3"]}}, "null": {}}
    });
    let error = serde_json::from_value::<VariantsJsonContent>(invalid)
        .expect_err("variant namespaces must appear in the priorities");
    assert_snapshot!(
        error,
        @"Variant namespace `cpu` is missing from default-priorities.namespace"
    );
}

#[test]
fn reject_invalid_variant_label() {
    let invalid = json!({
        "$schema": PEP_825_SCHEMA,
        "default-priorities": {"namespace": ["cpu"]},
        "variants": {"Bad": {}}
    });
    let error = serde_json::from_value::<VariantsJsonContent>(invalid)
        .expect_err("variant labels must be valid");
    assert_snapshot!(
        error,
        @"Variant label must contain only lowercase ASCII letters, digits, underscores, and periods, not `B`"
    );
}

#[test]
fn reject_invalid_variant_namespace() {
    let invalid = json!({
        "$schema": PEP_825_SCHEMA,
        "default-priorities": {"namespace": ["cpu"]},
        "variants": {"fast": {"CPU": {"level": ["v3"]}}, "null": {}}
    });
    let error = serde_json::from_value::<VariantsJsonContent>(invalid)
        .expect_err("variant namespaces must be valid");
    assert_snapshot!(error, @"Invalid variant namespace: `CPU`");
}

#[test]
fn reject_invalid_variant_feature() {
    let invalid = json!({
        "$schema": PEP_825_SCHEMA,
        "default-priorities": {"namespace": ["cpu"]},
        "variants": {"fast": {"cpu": {"bad.feature": ["v3"]}}, "null": {}}
    });
    let error = serde_json::from_value::<VariantsJsonContent>(invalid)
        .expect_err("variant features must be valid");
    assert_snapshot!(error, @"Invalid variant feature: `bad.feature`");
}

#[test]
fn reject_invalid_variant_value() {
    let invalid = json!({
        "$schema": PEP_825_SCHEMA,
        "default-priorities": {"namespace": ["cpu"]},
        "variants": {"fast": {"cpu": {"level": ["v 3"]}}, "null": {}}
    });
    let error = serde_json::from_value::<VariantsJsonContent>(invalid)
        .expect_err("variant values must be valid");
    assert_snapshot!(error, @"Invalid variant value: `v 3`");
}

#[test]
fn reject_empty_feature_values() {
    let invalid = json!({
        "$schema": PEP_825_SCHEMA,
        "default-priorities": {"namespace": ["cpu"]},
        "variants": {"fast": {"cpu": {"level": []}}, "null": {}}
    });
    let error = serde_json::from_value::<VariantsJsonContent>(invalid)
        .expect_err("variant features must have values");
    assert_snapshot!(error, @"Variant feature `cpu :: level` must have at least one value");
}

#[test]
fn reject_duplicate_feature_values() {
    let invalid = json!({
        "$schema": PEP_825_SCHEMA,
        "default-priorities": {"namespace": ["cpu"]},
        "variants": {"fast": {"cpu": {"level": ["v3", "v2", "v3"]}}, "null": {}}
    });
    let error = serde_json::from_value::<VariantsJsonContent>(invalid)
        .expect_err("variant values must be unique");
    assert_snapshot!(
        error,
        @r#"Variant feature `cpu :: level` must not contain duplicate values: ["v3", "v2", "v3"]"#
    );
}

#[test]
fn reject_null_variant_with_properties() {
    let invalid = json!({
        "$schema": PEP_825_SCHEMA,
        "default-priorities": {"namespace": ["cpu"]},
        "variants": {"fast": {"cpu": {"level": ["v2", "v3"]}}, "null": {"cpu": {"level": ["v3"]}}}
    });
    let error = serde_json::from_value::<VariantsJsonContent>(invalid)
        .expect_err("the null variant must not have properties");
    assert_snapshot!(error, @"The null variant must not have properties");
}

#[test]
fn reject_unknown_field_in_default_priorities() {
    let invalid = json!({
        "$schema": PEP_825_SCHEMA,
        "default-priorities": {"namespace": ["cpu"], "unknown": []},
        "variants": {"fast": {"cpu": {"level": ["v2", "v3"]}}, "null": {}}
    });
    let error = serde_json::from_value::<VariantsJsonContent>(invalid)
        .expect_err("default priorities must not contain unknown fields");
    assert_snapshot!(error, @"unknown field `unknown`, expected `namespace`");
}

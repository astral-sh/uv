use insta::{assert_json_snapshot, assert_snapshot};
use serde_json::{Value, json};

use uv_distribution_types::VariantsJsonContent;

const VARIANT_SCHEMA: &str = "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json";

/// Empty namespaces are removed during validation.
#[test]
fn normalize_variants_json() -> Result<(), Box<dyn std::error::Error>> {
    let metadata = json!({
        "$schema": VARIANT_SCHEMA,
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

fn valid_variants_json() -> Value {
    json!({
        "$schema": VARIANT_SCHEMA,
        "default-priorities": {"namespace": ["cpu"]},
        "variants": {"fast": {"cpu": {"level": ["v2", "v3"]}}, "null": {}}
    })
}

#[test]
fn reject_empty_namespace_priorities() {
    let mut invalid = valid_variants_json();
    invalid["default-priorities"]["namespace"] = json!([]);
    let error = serde_json::from_value::<VariantsJsonContent>(invalid).unwrap_err();
    assert_snapshot!(error, @"Variant namespace priorities must be non-empty and unique");
}

#[test]
fn reject_duplicate_namespace_priorities() {
    let mut invalid = valid_variants_json();
    invalid["default-priorities"]["namespace"] = json!(["cpu", "cpu"]);
    let error = serde_json::from_value::<VariantsJsonContent>(invalid).unwrap_err();
    assert_snapshot!(error, @"Variant namespace priorities must be non-empty and unique");
}

#[test]
fn reject_invalid_namespace_in_priorities() {
    let mut invalid = valid_variants_json();
    invalid["default-priorities"]["namespace"] = json!(["CPU"]);
    let error = serde_json::from_value::<VariantsJsonContent>(invalid).unwrap_err();
    assert_snapshot!(error, @"Invalid variant namespace: `CPU`");
}

#[test]
fn reject_namespace_missing_from_priorities() {
    let mut invalid = valid_variants_json();
    invalid["default-priorities"]["namespace"] = json!(["gpu"]);
    let error = serde_json::from_value::<VariantsJsonContent>(invalid).unwrap_err();
    assert_snapshot!(
        error,
        @"Variant namespace `cpu` is missing from default-priorities.namespace"
    );
}

#[test]
fn reject_invalid_variant_label() {
    let mut invalid = valid_variants_json();
    invalid["variants"] = json!({"Bad": {}});
    let error = serde_json::from_value::<VariantsJsonContent>(invalid).unwrap_err();
    assert_snapshot!(
        error,
        @"Variant label must contain only lowercase ASCII letters, digits, underscores, and periods, not `B`"
    );
}

#[test]
fn reject_invalid_variant_namespace() {
    let mut invalid = valid_variants_json();
    invalid["variants"]["fast"] = json!({"CPU": {"level": ["v3"]}});
    let error = serde_json::from_value::<VariantsJsonContent>(invalid).unwrap_err();
    assert_snapshot!(error, @"Invalid variant namespace: `CPU`");
}

#[test]
fn reject_invalid_variant_feature() {
    let mut invalid = valid_variants_json();
    invalid["variants"]["fast"]["cpu"] = json!({"bad.feature": ["v3"]});
    let error = serde_json::from_value::<VariantsJsonContent>(invalid).unwrap_err();
    assert_snapshot!(error, @"Invalid variant feature: `bad.feature`");
}

#[test]
fn reject_invalid_variant_value() {
    let mut invalid = valid_variants_json();
    invalid["variants"]["fast"]["cpu"]["level"] = json!(["v 3"]);
    let error = serde_json::from_value::<VariantsJsonContent>(invalid).unwrap_err();
    assert_snapshot!(error, @"Invalid variant value: `v 3`");
}

#[test]
fn reject_empty_feature_values() {
    let mut invalid = valid_variants_json();
    invalid["variants"]["fast"]["cpu"]["level"] = json!([]);
    let error = serde_json::from_value::<VariantsJsonContent>(invalid).unwrap_err();
    assert_snapshot!(error, @"Variant feature `cpu :: level` must have at least one value");
}

#[test]
fn reject_duplicate_feature_values() {
    let mut invalid = valid_variants_json();
    invalid["variants"]["fast"]["cpu"]["level"] = json!(["v3", "v2", "v3"]);
    let error = serde_json::from_value::<VariantsJsonContent>(invalid).unwrap_err();
    assert_snapshot!(error, @"Variant feature `cpu :: level` must not contain duplicate values");
}

#[test]
fn reject_null_variant_with_properties() {
    let mut invalid = valid_variants_json();
    invalid["variants"]["null"] = json!({"cpu": {"level": ["v3"]}});
    let error = serde_json::from_value::<VariantsJsonContent>(invalid).unwrap_err();
    assert_snapshot!(error, @"The null variant must not have properties");
}

#[test]
fn reject_unknown_field_in_default_priorities() {
    let mut invalid = valid_variants_json();
    invalid["default-priorities"]["unknown"] = json!([]);
    let error = serde_json::from_value::<VariantsJsonContent>(invalid).unwrap_err();
    assert_snapshot!(error, @"unknown field `unknown`, expected `namespace`");
}

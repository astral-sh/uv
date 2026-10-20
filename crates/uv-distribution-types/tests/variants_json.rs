use insta::{assert_json_snapshot, assert_snapshot};
use serde_json::json;

use uv_distribution_types::VariantsJsonContent;

/// Empty namespaces are removed during validation.
#[test]
fn normalize_variants_json() -> Result<(), Box<dyn std::error::Error>> {
    let metadata = json!({
        "$schema": "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json",
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
        "$schema": "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json",
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
        "$schema": "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json",
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
        "$schema": "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json",
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
        "$schema": "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json",
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
        "$schema": "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json",
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
        "$schema": "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json",
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
        "$schema": "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json",
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
        "$schema": "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json",
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
        "$schema": "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json",
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
        "$schema": "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json",
        "default-priorities": {"namespace": ["cpu"]},
        "variants": {"fast": {"cpu": {"level": ["v3", "v2", "v3"]}}, "null": {}}
    });
    let error = serde_json::from_value::<VariantsJsonContent>(invalid)
        .expect_err("variant values must be unique");
    assert_snapshot!(error, @"Variant feature `cpu :: level` must not contain duplicate values");
}

#[test]
fn reject_null_variant_with_properties() {
    let invalid = json!({
        "$schema": "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json",
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
        "$schema": "https://variants-schema.wheelnext.dev/peps/825/v0.1.1.json",
        "default-priorities": {"namespace": ["cpu"], "unknown": []},
        "variants": {"fast": {"cpu": {"level": ["v2", "v3"]}}, "null": {}}
    });
    let error = serde_json::from_value::<VariantsJsonContent>(invalid)
        .expect_err("default priorities must not contain unknown fields");
    assert_snapshot!(error, @"unknown field `unknown`, expected `namespace`");
}

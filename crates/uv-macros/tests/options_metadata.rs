use uv_macros::OptionsMetadata;
use uv_options_metadata::{OptionEntry, OptionField, OptionsMetadata};

#[derive(serde::Deserialize, OptionsMetadata)]
#[allow(dead_code)]
struct RootOptions {
    #[serde(flatten)]
    nested: NestedOptions,
}

#[derive(serde::Deserialize, OptionsMetadata)]
#[allow(dead_code)]
struct NestedOptions {
    /// Whether to enable the child option.
    #[option(
        default = "false",
        value_type = "bool",
        example = "child-option = true"
    )]
    child_option: Option<bool>,
}

#[test]
fn options_metadata_flattens_serde_fields() {
    assert_eq!(
        RootOptions::metadata().find("child-option"),
        Some(OptionEntry::Field(OptionField {
            doc: "Whether to enable the child option.",
            default: "false",
            value_type: "bool",
            scope: None,
            example: "child-option = true",
            deprecated: None,
            possible_values: None,
            uv_toml_only: false,
        }))
    );
}

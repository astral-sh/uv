//! Tests for validation of the `preview` and `preview-features` settings.
//!
//! In particular, a non-boolean `preview` value (e.g. a `[tool.uv.preview]` table with an
//! unknown key, like `nonexistent = true`) must produce a clear error that reports the unknown
//! preview feature, instead of being silently ignored. See
//! <https://github.com/astral-sh/uv/issues/22950>.

use uv_settings::{Options, PreviewFeaturesOption, PreviewOption};

/// Parse the given `[tool.uv]` contents, returning the parse error message on failure.
fn parse_error(toml_str: &str) -> String {
    toml::from_str::<Options>(toml_str)
        .expect_err("expected the `preview` setting to fail validation")
        .to_string()
}

#[test]
fn preview_boolean() {
    let options = toml::from_str::<Options>("preview = true").unwrap();
    assert!(matches!(
        options.globals.preview,
        Some(PreviewOption::Preview(true))
    ));

    let options = toml::from_str::<Options>("preview = false").unwrap();
    assert!(matches!(
        options.globals.preview,
        Some(PreviewOption::Preview(false))
    ));
}

#[test]
fn preview_features_toggle() {
    let options = toml::from_str::<Options>("preview-features = true").unwrap();
    assert!(matches!(
        options.globals.preview,
        Some(PreviewOption::PreviewFeatures(
            PreviewFeaturesOption::Toggle(true)
        ))
    ));
}

#[test]
fn preview_features_list() {
    let options = toml::from_str::<Options>(r#"preview-features = ["json-output"]"#).unwrap();
    assert!(matches!(
        options.globals.preview,
        Some(PreviewOption::PreviewFeatures(
            PreviewFeaturesOption::Features(_)
        ))
    ));
}

#[test]
fn preview_features_unknown_name_is_accepted() {
    // Unknown preview feature names in `preview-features` are accepted at parse time for
    // forwards compatibility (e.g. a feature that was removed after being stabilized); they are
    // reported as a warning when the preview configuration is resolved.
    let options = toml::from_str::<Options>(r#"preview-features = ["nonexistent"]"#).unwrap();
    assert!(matches!(
        options.globals.preview,
        Some(PreviewOption::PreviewFeatures(
            PreviewFeaturesOption::Features(_)
        ))
    ));
}

#[test]
fn preview_table_with_unknown_key_errors() {
    // e.g. `[tool.uv.preview]` with `nonexistent = true`.
    let message = parse_error("[preview]\nnonexistent = true\n");
    assert!(
        message.contains("unknown preview feature `nonexistent`"),
        "unexpected error message: {message}"
    );
    assert!(
        message.contains("`preview-features`"),
        "error should point to `preview-features`: {message}"
    );
}

#[test]
fn preview_table_with_known_feature_errors() {
    // e.g. `[tool.uv.preview]` with `json-output = true`.
    let message = parse_error("[preview]\njson-output = true\n");
    assert!(
        message.contains("expected a boolean for `preview`"),
        "unexpected error message: {message}"
    );
    assert!(
        message.contains(r#"preview-features = ["json-output"]"#),
        "error should suggest `preview-features`: {message}"
    );
}

#[test]
fn preview_list_with_unknown_feature_errors() {
    let message = parse_error(r#"preview = ["nonexistent"]"#);
    assert!(
        message.contains("unknown preview feature `nonexistent`"),
        "unexpected error message: {message}"
    );
    assert!(
        message.contains("`preview-features`"),
        "error should point to `preview-features`: {message}"
    );
}

#[test]
fn preview_string_with_known_feature_errors() {
    let message = parse_error(r#"preview = "json-output""#);
    assert!(
        message.contains("expected a boolean for `preview`"),
        "unexpected error message: {message}"
    );
    assert!(
        message.contains(r#"preview-features = ["json-output"]"#),
        "error should suggest `preview-features`: {message}"
    );
}

#[test]
fn preview_string_with_unknown_value_errors() {
    let message = parse_error(r#"preview = "nonexistent""#);
    assert!(
        message.contains("invalid type: string `nonexistent`, expected a boolean for `preview`"),
        "unexpected error message: {message}"
    );
    assert!(
        message.contains("`preview-features`"),
        "error should point to `preview-features`: {message}"
    );
}

#[test]
fn preview_invalid_type_errors() {
    let message = parse_error("preview = 1");
    assert!(
        message.contains("expected a boolean for `preview`"),
        "unexpected error message: {message}"
    );
}

#[test]
fn preview_and_preview_features_conflict() {
    let message = parse_error("preview = true\npreview-features = [\"json-output\"]\n");
    assert!(
        message.contains("cannot specify both `preview` and `preview-features`"),
        "unexpected error message: {message}"
    );
}

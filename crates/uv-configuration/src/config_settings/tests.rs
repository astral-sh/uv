use super::*;

#[test]
fn collect_config_settings() {
    let settings: ConfigSettings = vec![
        ConfigSettingEntry {
            key: "key".to_string(),
            value: "value".to_string(),
        },
        ConfigSettingEntry {
            key: "key".to_string(),
            value: "value2".to_string(),
        },
        ConfigSettingEntry {
            key: "list".to_string(),
            value: "value3".to_string(),
        },
        ConfigSettingEntry {
            key: "list".to_string(),
            value: "value4".to_string(),
        },
    ]
    .into_iter()
    .collect();
    assert_eq!(
        settings.0.get("key"),
        Some(&ConfigSettingValue::List(vec![
            "value".to_string(),
            "value2".to_string()
        ]))
    );
    assert_eq!(
        settings.0.get("list"),
        Some(&ConfigSettingValue::List(vec![
            "value3".to_string(),
            "value4".to_string()
        ]))
    );
}

#[test]
fn escape_for_python() {
    let mut settings = ConfigSettings::default();
    settings.0.insert(
        "key".to_string(),
        ConfigSettingValue::String("value".to_string()),
    );
    settings.0.insert(
        "list".to_string(),
        ConfigSettingValue::List(vec!["value1".to_string(), "value2".to_string()]),
    );
    assert_eq!(
        settings.escape_for_python(),
        r#"{"key":"value","list":["value1","value2"]}"#
    );

    let mut settings = ConfigSettings::default();
    settings.0.insert(
        "key".to_string(),
        ConfigSettingValue::String("Hello, \"world!\"".to_string()),
    );
    settings.0.insert(
        "list".to_string(),
        ConfigSettingValue::List(vec!["'value1'".to_string()]),
    );
    assert_eq!(
        settings.escape_for_python(),
        r#"{"key":"Hello, \"world!\"","list":["'value1'"]}"#
    );

    let mut settings = ConfigSettings::default();
    settings.0.insert(
        "key".to_string(),
        ConfigSettingValue::String("val\\1 {}value".to_string()),
    );
    assert_eq!(settings.escape_for_python(), r#"{"key":"val\\1 {}value"}"#);
}

use super::*;

#[test]
fn test_has_child_option() {
    struct WithOptions;

    impl OptionsMetadata for WithOptions {
        fn record(visit: &mut dyn Visit) {
            visit.record_field(
                "ignore-git-ignore",
                OptionField {
                    doc: "Whether Ruff should respect the gitignore file",
                    default: "false",
                    value_type: "bool",
                    example: "",
                    scope: None,
                    deprecated: None,
                    possible_values: None,
                },
            );
        }
    }

    assert!(WithOptions::metadata().has("ignore-git-ignore"));
    assert!(!WithOptions::metadata().has("does-not-exist"));
}

#[test]
fn test_has_nested_option() {
    struct Root;

    impl OptionsMetadata for Root {
        fn record(visit: &mut dyn Visit) {
            visit.record_field(
                "ignore-git-ignore",
                OptionField {
                    doc: "Whether Ruff should respect the gitignore file",
                    default: "false",
                    value_type: "bool",
                    example: "",
                    scope: None,
                    deprecated: None,
                    possible_values: None,
                },
            );

            visit.record_set("format", Nested::metadata());
        }
    }

    struct Nested;

    impl OptionsMetadata for Nested {
        fn record(visit: &mut dyn Visit) {
            visit.record_field(
                "hard-tabs",
                OptionField {
                    doc: "Use hard tabs for indentation and spaces for alignment.",
                    default: "false",
                    value_type: "bool",
                    example: "",
                    scope: None,
                    deprecated: None,
                    possible_values: None,
                },
            );
        }
    }

    assert!(Root::metadata().has("format.hard-tabs"));
    assert!(!Root::metadata().has("format.spaces"));
    assert!(!Root::metadata().has("lint.hard-tabs"));
}

#[test]
fn test_find_child_option() {
    struct WithOptions;

    static IGNORE_GIT_IGNORE: OptionField = OptionField {
        doc: "Whether Ruff should respect the gitignore file",
        default: "false",
        value_type: "bool",
        example: "",
        scope: None,
        deprecated: None,
        possible_values: None,
    };

    impl OptionsMetadata for WithOptions {
        fn record(visit: &mut dyn Visit) {
            visit.record_field("ignore-git-ignore", IGNORE_GIT_IGNORE.clone());
        }
    }

    assert_eq!(
        WithOptions::metadata().find("ignore-git-ignore"),
        Some(OptionEntry::Field(IGNORE_GIT_IGNORE.clone()))
    );
    assert_eq!(WithOptions::metadata().find("does-not-exist"), None);
}

#[test]
fn test_find_nested_option() {
    static HARD_TABS: OptionField = OptionField {
        doc: "Use hard tabs for indentation and spaces for alignment.",
        default: "false",
        value_type: "bool",
        example: "",
        scope: None,
        deprecated: None,
        possible_values: None,
    };

    struct Root;

    impl OptionsMetadata for Root {
        fn record(visit: &mut dyn Visit) {
            visit.record_field(
                "ignore-git-ignore",
                OptionField {
                    doc: "Whether Ruff should respect the gitignore file",
                    default: "false",
                    value_type: "bool",
                    example: "",
                    scope: None,
                    deprecated: None,
                    possible_values: None,
                },
            );

            visit.record_set("format", Nested::metadata());
        }
    }

    struct Nested;

    impl OptionsMetadata for Nested {
        fn record(visit: &mut dyn Visit) {
            visit.record_field("hard-tabs", HARD_TABS.clone());
        }
    }

    assert_eq!(
        Root::metadata().find("format.hard-tabs"),
        Some(OptionEntry::Field(HARD_TABS.clone()))
    );
    assert_eq!(
        Root::metadata().find("format"),
        Some(OptionEntry::Set(Nested::metadata()))
    );
    assert_eq!(Root::metadata().find("format.spaces"), None);
    assert_eq!(Root::metadata().find("lint.hard-tabs"), None);
}

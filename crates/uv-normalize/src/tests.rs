use super::*;

#[test]
fn normalize() {
    let inputs = [
        "friendly-bard",
        "Friendly-Bard",
        "FRIENDLY-BARD",
        "friendly.bard",
        "friendly_bard",
        "friendly--bard",
        "friendly-.bard",
        "FrIeNdLy-._.-bArD",
    ];
    for input in inputs {
        assert_eq!(validate_and_normalize_ref(input).unwrap(), "friendly-bard");
        assert_eq!(
            validate_and_normalize_owned(input.to_string()).unwrap(),
            "friendly-bard"
        );
    }
}

#[test]
fn check() {
    let inputs = ["friendly-bard", "friendlybard"];
    for input in inputs {
        assert!(is_normalized(input).unwrap(), "{input:?}");
    }

    let inputs = [
        "friendly.bard",
        "friendly.BARD",
        "friendly_bard",
        "friendly--bard",
        "friendly-.bard",
        "FrIeNdLy-._.-bArD",
    ];
    for input in inputs {
        assert!(!is_normalized(input).unwrap(), "{input:?}");
    }
}

#[test]
fn unchanged() {
    // Unchanged
    let unchanged = ["friendly-bard", "1okay", "okay2"];
    for input in unchanged {
        assert_eq!(validate_and_normalize_ref(input).unwrap(), input);
        assert_eq!(
            validate_and_normalize_owned(input.to_string()).unwrap(),
            input
        );
        assert!(is_normalized(input).unwrap());
    }
}

#[test]
fn failures() {
    let failures = [
        " starts-with-space",
        "-starts-with-dash",
        "ends-with-dash-",
        "ends-with-space ",
        "includes!invalid-char",
        "space in middle",
        "alpha-α",
    ];
    for input in failures {
        assert!(validate_and_normalize_ref(input).is_err());
        assert!(validate_and_normalize_owned(input.to_string()).is_err());
        assert!(is_normalized(input).is_err());
    }
}

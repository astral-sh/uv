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
        assert_eq!(DistInfoName::normalize(input), "friendly-bard");
    }
}

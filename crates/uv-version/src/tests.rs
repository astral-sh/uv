use super::*;

#[test]
fn test_get_version() {
    assert_eq!(version().to_string(), env!("CARGO_PKG_VERSION").to_string());
}

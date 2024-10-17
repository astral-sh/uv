use super::{Version, VersionSpecifier, VersionSpecifiers};
use std::str::FromStr;

#[test]
fn test_version() {
    let version = Version::from_str("1.19").unwrap();
    let version_specifier = VersionSpecifier::from_str("== 1.*").unwrap();
    assert!(version_specifier.contains(&version));
    let version_specifiers = VersionSpecifiers::from_str(">=1.16, <2.0").unwrap();
    assert!(version_specifiers.contains(&version));
}

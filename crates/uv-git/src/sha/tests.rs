use std::str::FromStr;

use super::{GitOid, OidParseError};

#[test]
fn git_oid() {
    GitOid::from_str("4a23745badf5bf5ef7928f1e346e9986bd696d82").unwrap();

    assert_eq!(GitOid::from_str(""), Err(OidParseError::Empty));
    assert_eq!(
        GitOid::from_str(&str::repeat("a", 41)),
        Err(OidParseError::TooLong)
    );
}

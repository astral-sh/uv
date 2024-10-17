use std::str::FromStr;

use uv_pep440::{Prerelease, PrereleaseKind, Version};

use crate::PythonVersion;

#[test]
fn python_markers() {
    let version = PythonVersion::from_str("3.11.0").expect("valid python version");
    assert_eq!(version.python_version(), Version::new([3, 11]));
    assert_eq!(version.python_version().to_string(), "3.11");
    assert_eq!(version.python_full_version(), Version::new([3, 11, 0]));
    assert_eq!(version.python_full_version().to_string(), "3.11.0");

    let version = PythonVersion::from_str("3.11").expect("valid python version");
    assert_eq!(version.python_version(), Version::new([3, 11]));
    assert_eq!(version.python_version().to_string(), "3.11");
    assert_eq!(version.python_full_version(), Version::new([3, 11, 0]));
    assert_eq!(version.python_full_version().to_string(), "3.11.0");

    let version = PythonVersion::from_str("3.11.8a1").expect("valid python version");
    assert_eq!(version.python_version(), Version::new([3, 11]));
    assert_eq!(version.python_version().to_string(), "3.11");
    assert_eq!(
        version.python_full_version(),
        Version::new([3, 11, 8]).with_pre(Some(Prerelease {
            kind: PrereleaseKind::Alpha,
            number: 1
        }))
    );
    assert_eq!(version.python_full_version().to_string(), "3.11.8a1");
}

use std::str::FromStr;

use uv_normalize::PackageName;

use crate::{SourceDistExtension, SourceDistFilename};

/// Only test already normalized names since the parsing is lossy
///
/// <https://packaging.python.org/en/latest/specifications/source-distribution-format/#source-distribution-file-name>
/// <https://packaging.python.org/en/latest/specifications/binary-distribution-format/#escaping-and-unicode>
#[test]
fn roundtrip() {
    for normalized in [
        "foo_lib-1.2.3.zip",
        "foo_lib-1.2.3a3.zip",
        "foo_lib-1.2.3.tar.gz",
        "foo_lib-1.2.3.tar.bz2",
        "foo_lib-1.2.3.tar.zst",
    ] {
        let ext = SourceDistExtension::from_path(normalized).unwrap();
        assert_eq!(
            SourceDistFilename::parse(normalized, ext, &PackageName::from_str("foo_lib").unwrap())
                .unwrap()
                .to_string(),
            normalized
        );
    }
}

#[test]
fn errors() {
    for invalid in ["b-1.2.3.zip", "a-1.2.3-gamma.3.zip"] {
        let ext = SourceDistExtension::from_path(invalid).unwrap();
        assert!(
            SourceDistFilename::parse(invalid, ext, &PackageName::from_str("a").unwrap()).is_err()
        );
    }
}

#[test]
fn name_too_long() {
    assert!(SourceDistFilename::parse(
        "foo.zip",
        SourceDistExtension::Zip,
        &PackageName::from_str("foo-lib").unwrap()
    )
    .is_err());
}

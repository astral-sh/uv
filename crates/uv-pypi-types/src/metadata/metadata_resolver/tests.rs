use super::*;
use crate::MetadataError;
use std::str::FromStr;
use uv_normalize::PackageName;
use uv_pep440::Version;

#[test]
fn test_parse_metadata() {
    let s = "Metadata-Version: 1.0";
    let meta = ResolutionMetadata::parse_metadata(s.as_bytes());
    assert!(matches!(meta, Err(MetadataError::FieldNotFound("Name"))));

    let s = "Metadata-Version: 1.0\nName: asdf";
    let meta = ResolutionMetadata::parse_metadata(s.as_bytes());
    assert!(matches!(meta, Err(MetadataError::FieldNotFound("Version"))));

    let s = "Metadata-Version: 1.0\nName: asdf\nVersion: 1.0";
    let meta = ResolutionMetadata::parse_metadata(s.as_bytes()).unwrap();
    assert_eq!(meta.name, PackageName::from_str("asdf").unwrap());
    assert_eq!(meta.version, Version::new([1, 0]));

    let s = "Metadata-Version: 1.0\nName: asdf\nVersion: 1.0\nAuthor: 中文\n\n一个 Python 包";
    let meta = ResolutionMetadata::parse_metadata(s.as_bytes()).unwrap();
    assert_eq!(meta.name, PackageName::from_str("asdf").unwrap());
    assert_eq!(meta.version, Version::new([1, 0]));

    let s = "Metadata-Version: 1.0\nName: =?utf-8?q?foobar?=\nVersion: 1.0";
    let meta = ResolutionMetadata::parse_metadata(s.as_bytes()).unwrap();
    assert_eq!(meta.name, PackageName::from_str("foobar").unwrap());
    assert_eq!(meta.version, Version::new([1, 0]));

    let s = "Metadata-Version: 1.0\nName: =?utf-8?q?=C3=A4_space?= <x@y.org>\nVersion: 1.0";
    let meta = ResolutionMetadata::parse_metadata(s.as_bytes());
    assert!(matches!(meta, Err(MetadataError::InvalidName(_))));
}

#[test]
fn test_parse_pkg_info() {
    let s = "Metadata-Version: 2.1";
    let meta = ResolutionMetadata::parse_pkg_info(s.as_bytes());
    assert!(matches!(
        meta,
        Err(MetadataError::UnsupportedMetadataVersion(_))
    ));

    let s = "Metadata-Version: 2.2\nName: asdf";
    let meta = ResolutionMetadata::parse_pkg_info(s.as_bytes());
    assert!(matches!(meta, Err(MetadataError::FieldNotFound("Version"))));

    let s = "Metadata-Version: 2.3\nName: asdf";
    let meta = ResolutionMetadata::parse_pkg_info(s.as_bytes());
    assert!(matches!(meta, Err(MetadataError::FieldNotFound("Version"))));

    let s = "Metadata-Version: 2.3\nName: asdf\nVersion: 1.0";
    let meta = ResolutionMetadata::parse_pkg_info(s.as_bytes()).unwrap();
    assert_eq!(meta.name, PackageName::from_str("asdf").unwrap());
    assert_eq!(meta.version, Version::new([1, 0]));

    let s = "Metadata-Version: 2.3\nName: asdf\nVersion: 1.0\nDynamic: Requires-Dist";
    let meta = ResolutionMetadata::parse_pkg_info(s.as_bytes()).unwrap_err();
    assert!(matches!(meta, MetadataError::DynamicField("Requires-Dist")));

    let s = "Metadata-Version: 2.3\nName: asdf\nVersion: 1.0\nRequires-Dist: foo";
    let meta = ResolutionMetadata::parse_pkg_info(s.as_bytes()).unwrap();
    assert_eq!(meta.name, PackageName::from_str("asdf").unwrap());
    assert_eq!(meta.version, Version::new([1, 0]));
    assert_eq!(meta.requires_dist, vec!["foo".parse().unwrap()]);
}

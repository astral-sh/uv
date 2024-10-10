use super::*;
use crate::MetadataError;

#[test]
fn test_parse_from_str() {
    let s = "Metadata-Version: 1.0";
    let meta: Result<Metadata23, MetadataError> = s.parse();
    assert!(matches!(meta, Err(MetadataError::FieldNotFound("Name"))));

    let s = "Metadata-Version: 1.0\nName: asdf";
    let meta = Metadata23::parse(s.as_bytes());
    assert!(matches!(meta, Err(MetadataError::FieldNotFound("Version"))));

    let s = "Metadata-Version: 1.0\nName: asdf\nVersion: 1.0";
    let meta = Metadata23::parse(s.as_bytes()).unwrap();
    assert_eq!(meta.metadata_version, "1.0");
    assert_eq!(meta.name, "asdf");
    assert_eq!(meta.version, "1.0");

    let s = "Metadata-Version: 1.0\nName: asdf\nVersion: 1.0\nDescription: a Python package";
    let meta: Metadata23 = s.parse().unwrap();
    assert_eq!(meta.description.as_deref(), Some("a Python package"));

    let s = "Metadata-Version: 1.0\nName: asdf\nVersion: 1.0\n\na Python package";
    let meta: Metadata23 = s.parse().unwrap();
    assert_eq!(meta.description.as_deref(), Some("a Python package"));

    let s = "Metadata-Version: 1.0\nName: asdf\nVersion: 1.0\nAuthor: 中文\n\n一个 Python 包";
    let meta: Metadata23 = s.parse().unwrap();
    assert_eq!(meta.author.as_deref(), Some("中文"));
    assert_eq!(meta.description.as_deref(), Some("一个 Python 包"));
}

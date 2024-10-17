use crate::metadata::pyproject_toml::parse_pyproject_toml;
use crate::MetadataError;
use std::str::FromStr;
use uv_normalize::PackageName;
use uv_pep440::Version;

#[test]
fn test_parse_pyproject_toml() {
    let s = r#"
        [project]
        name = "asdf"
    "#;
    let meta = parse_pyproject_toml(s);
    assert!(matches!(meta, Err(MetadataError::FieldNotFound("version"))));

    let s = r#"
        [project]
        name = "asdf"
        dynamic = ["version"]
    "#;
    let meta = parse_pyproject_toml(s);
    assert!(matches!(meta, Err(MetadataError::DynamicField("version"))));

    let s = r#"
        [project]
        name = "asdf"
        version = "1.0"
    "#;
    let meta = parse_pyproject_toml(s).unwrap();
    assert_eq!(meta.name, PackageName::from_str("asdf").unwrap());
    assert_eq!(meta.version, Version::new([1, 0]));
    assert!(meta.requires_python.is_none());
    assert!(meta.requires_dist.is_empty());
    assert!(meta.provides_extras.is_empty());

    let s = r#"
        [project]
        name = "asdf"
        version = "1.0"
        requires-python = ">=3.6"
    "#;
    let meta = parse_pyproject_toml(s).unwrap();
    assert_eq!(meta.name, PackageName::from_str("asdf").unwrap());
    assert_eq!(meta.version, Version::new([1, 0]));
    assert_eq!(meta.requires_python, Some(">=3.6".parse().unwrap()));
    assert!(meta.requires_dist.is_empty());
    assert!(meta.provides_extras.is_empty());

    let s = r#"
        [project]
        name = "asdf"
        version = "1.0"
        requires-python = ">=3.6"
        dependencies = ["foo"]
    "#;
    let meta = parse_pyproject_toml(s).unwrap();
    assert_eq!(meta.name, PackageName::from_str("asdf").unwrap());
    assert_eq!(meta.version, Version::new([1, 0]));
    assert_eq!(meta.requires_python, Some(">=3.6".parse().unwrap()));
    assert_eq!(meta.requires_dist, vec!["foo".parse().unwrap()]);
    assert!(meta.provides_extras.is_empty());

    let s = r#"
        [project]
        name = "asdf"
        version = "1.0"
        requires-python = ">=3.6"
        dependencies = ["foo"]

        [project.optional-dependencies]
        dotenv = ["bar"]
    "#;
    let meta = parse_pyproject_toml(s).unwrap();
    assert_eq!(meta.name, PackageName::from_str("asdf").unwrap());
    assert_eq!(meta.version, Version::new([1, 0]));
    assert_eq!(meta.requires_python, Some(">=3.6".parse().unwrap()));
    assert_eq!(
        meta.requires_dist,
        vec![
            "foo".parse().unwrap(),
            "bar; extra == \"dotenv\"".parse().unwrap()
        ]
    );
    assert_eq!(meta.provides_extras, vec!["dotenv".parse().unwrap()]);
}

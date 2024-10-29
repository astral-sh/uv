use std::path::PathBuf;

use uv_pep508::{MarkerTree, VerbatimUrl};

use crate::{Requirement, RequirementSource};

#[test]
fn roundtrip() {
    let requirement = Requirement {
        name: "foo".parse().unwrap(),
        extras: vec![],
        marker: MarkerTree::TRUE,
        source: RequirementSource::Registry {
            specifier: ">1,<2".parse().unwrap(),
            index: None,
        },
        origin: None,
    };

    let raw = toml::to_string(&requirement).unwrap();
    let deserialized: Requirement = toml::from_str(&raw).unwrap();
    assert_eq!(requirement, deserialized);

    let path = if cfg!(windows) {
        "C:\\home\\ferris\\foo"
    } else {
        "/home/ferris/foo"
    };
    let requirement = Requirement {
        name: "foo".parse().unwrap(),
        extras: vec![],
        marker: MarkerTree::TRUE,
        source: RequirementSource::Directory {
            install_path: PathBuf::from(path),
            editable: false,
            r#virtual: false,
            url: VerbatimUrl::from_absolute_path(path).unwrap(),
        },
        origin: None,
    };

    let raw = toml::to_string(&requirement).unwrap();
    let deserialized: Requirement = toml::from_str(&raw).unwrap();
    assert_eq!(requirement, deserialized);
}

use super::*;

#[test]
fn egg_info_filename() {
    let filename = "zstandard-0.22.0-py3.12-darwin.egg-info";
    let parsed = EggInfoFilename::from_str(filename).unwrap();
    assert_eq!(parsed.name.as_ref(), "zstandard");
    assert_eq!(
        parsed.version.map(|v| v.to_string()),
        Some("0.22.0".to_string())
    );

    let filename = "zstandard-0.22.0-py3.12.egg-info";
    let parsed = EggInfoFilename::from_str(filename).unwrap();
    assert_eq!(parsed.name.as_ref(), "zstandard");
    assert_eq!(
        parsed.version.map(|v| v.to_string()),
        Some("0.22.0".to_string())
    );

    let filename = "zstandard-0.22.0.egg-info";
    let parsed = EggInfoFilename::from_str(filename).unwrap();
    assert_eq!(parsed.name.as_ref(), "zstandard");
    assert_eq!(
        parsed.version.map(|v| v.to_string()),
        Some("0.22.0".to_string())
    );

    let filename = "zstandard.egg-info";
    let parsed = EggInfoFilename::from_str(filename).unwrap();
    assert_eq!(parsed.name.as_ref(), "zstandard");
    assert!(parsed.version.is_none());
}

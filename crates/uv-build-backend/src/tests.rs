use super::*;
use insta::{assert_snapshot, with_settings};
use std::str::FromStr;
use tempfile::TempDir;
use uv_normalize::PackageName;
use uv_pep440::Version;

#[test]
fn test_wheel() {
    let filename = WheelFilename {
        name: PackageName::from_str("foo").unwrap(),
        version: Version::from_str("1.2.3").unwrap(),
        build_tag: None,
        python_tag: vec!["py2".to_string(), "py3".to_string()],
        abi_tag: vec!["none".to_string()],
        platform_tag: vec!["any".to_string()],
    };

    with_settings!({
        filters => [(uv_version::version(), "[VERSION]")],
    }, {
        assert_snapshot!(wheel_info(&filename), @r"
                Wheel-Version: 1.0
                Generator: uv [VERSION]
                Root-Is-Purelib: true
                Tag: py2-none-any
                Tag: py3-none-any
            ");
    });
}

#[test]
fn test_record() {
    let record = vec![RecordEntry {
        path: "uv_backend/__init__.py".to_string(),
        hash: "89f869e53a3a0061a52c0233e6442d4d72de80a8a2d3406d9ea0bfd397ed7865".to_string(),
        size: 37,
    }];

    let mut writer = Vec::new();
    write_record(&mut writer, "uv_backend-0.1.0", record).unwrap();
    assert_snapshot!(String::from_utf8(writer).unwrap(), @r"
            uv_backend/__init__.py,sha256=89f869e53a3a0061a52c0233e6442d4d72de80a8a2d3406d9ea0bfd397ed7865,37
            uv_backend-0.1.0/RECORD,,
        ");
}

/// Check that we write deterministic wheels.
#[test]
fn test_determinism() {
    let temp1 = TempDir::new().unwrap();
    let uv_backend = Path::new("../../scripts/packages/uv_backend");
    build(uv_backend, temp1.path(), None).unwrap();

    // Touch the file to check that we don't serialize the last modified date.
    fs_err::write(
        uv_backend.join("src/uv_backend/__init__.py"),
        "def greet():\n    print(\"Hello 👋\")\n",
    )
    .unwrap();

    let temp2 = TempDir::new().unwrap();
    build(uv_backend, temp2.path(), None).unwrap();

    let wheel_filename = "uv_backend-0.1.0-py3-none-any.whl";
    assert_eq!(
        fs_err::read(temp1.path().join(wheel_filename)).unwrap(),
        fs_err::read(temp2.path().join(wheel_filename)).unwrap()
    );
}

/// Snapshot all files from the prepare metadata hook.
#[test]
fn test_prepare_metadata() {
    let metadata_dir = TempDir::new().unwrap();
    let uv_backend = Path::new("../../scripts/packages/uv_backend");
    metadata(uv_backend, metadata_dir.path()).unwrap();

    let mut files: Vec<_> = WalkDir::new(metadata_dir.path())
        .into_iter()
        .map(|entry| {
            entry
                .unwrap()
                .path()
                .strip_prefix(metadata_dir.path())
                .unwrap()
                .portable_display()
                .to_string()
        })
        .filter(|path| !path.is_empty())
        .collect();
    files.sort();
    assert_snapshot!(files.join("\n"), @r"
        uv_backend-0.1.0.dist-info
        uv_backend-0.1.0.dist-info/METADATA
        uv_backend-0.1.0.dist-info/RECORD
        uv_backend-0.1.0.dist-info/WHEEL
        ");

    let metadata_file = metadata_dir
        .path()
        .join("uv_backend-0.1.0.dist-info/METADATA");
    assert_snapshot!(fs_err::read_to_string(metadata_file).unwrap(), @r###"
        Metadata-Version: 2.3
        Name: uv-backend
        Version: 0.1.0
        Summary: Add your description here
        Requires-Python: >=3.12
        Description-Content-Type: text/markdown

        # uv_backend

        A simple package to be built with the uv build backend.
        "###);

    let record_file = metadata_dir
        .path()
        .join("uv_backend-0.1.0.dist-info/RECORD");
    assert_snapshot!(fs_err::read_to_string(record_file).unwrap(), @r###"
    uv_backend-0.1.0.dist-info/WHEEL,sha256=1889a32410898a395359e85de53e3063be35d9113130eaf23659cb84740c0c6c,79
    uv_backend-0.1.0.dist-info/METADATA,sha256=e4a0d390317d7182f65ea978254c71ed283e0a4242150cf1c99a694b113ff68d,224
    uv_backend-0.1.0.dist-info/RECORD,,
    "###);

    let wheel_file = metadata_dir.path().join("uv_backend-0.1.0.dist-info/WHEEL");
    let filters = vec![(uv_version::version(), "[VERSION]")];
    with_settings!({
        filters => filters
    }, {
        assert_snapshot!(fs_err::read_to_string(wheel_file).unwrap(), @r###"
                Wheel-Version: 1.0
                Generator: uv [VERSION]
                Root-Is-Purelib: true
                Tag: py3-none-any
            "###);
    });
}

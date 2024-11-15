use super::*;
use flate2::bufread::GzDecoder;
use insta::assert_snapshot;
use std::str::FromStr;
use tempfile::TempDir;
use uv_fs::copy_dir_all;
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

    assert_snapshot!(wheel_info(&filename, "1.0.0+test"), @r"
        Wheel-Version: 1.0
        Generator: uv 1.0.0+test
        Root-Is-Purelib: true
        Tag: py2-none-any
        Tag: py3-none-any
    ");
}

#[test]
fn test_record() {
    let record = vec![RecordEntry {
        path: "built_by_uv/__init__.py".to_string(),
        hash: "89f869e53a3a0061a52c0233e6442d4d72de80a8a2d3406d9ea0bfd397ed7865".to_string(),
        size: 37,
    }];

    let mut writer = Vec::new();
    write_record(&mut writer, "built_by_uv-0.1.0", record).unwrap();
    assert_snapshot!(String::from_utf8(writer).unwrap(), @r"
            built_by_uv/__init__.py,sha256=89f869e53a3a0061a52c0233e6442d4d72de80a8a2d3406d9ea0bfd397ed7865,37
            built_by_uv-0.1.0/RECORD,,
        ");
}

/// Snapshot all files from the prepare metadata hook.
#[test]
fn test_prepare_metadata() {
    let metadata_dir = TempDir::new().unwrap();
    let built_by_uv = Path::new("../../scripts/packages/built-by-uv");
    metadata(built_by_uv, metadata_dir.path(), "1.0.0+test").unwrap();

    let mut files: Vec<_> = WalkDir::new(metadata_dir.path())
        .into_iter()
        .map(|entry| {
            entry
                .unwrap()
                .path()
                .strip_prefix(metadata_dir.path())
                .expect("walkdir starts with root")
                .portable_display()
                .to_string()
        })
        .filter(|path| !path.is_empty())
        .collect();
    files.sort();
    assert_snapshot!(files.join("\n"), @r"
        built_by_uv-0.1.0.dist-info
        built_by_uv-0.1.0.dist-info/METADATA
        built_by_uv-0.1.0.dist-info/RECORD
        built_by_uv-0.1.0.dist-info/WHEEL
        ");

    let metadata_file = metadata_dir
        .path()
        .join("built_by_uv-0.1.0.dist-info/METADATA");
    assert_snapshot!(fs_err::read_to_string(metadata_file).unwrap(), @r###"
    Metadata-Version: 2.4
    Name: built-by-uv
    Version: 0.1.0
    Summary: A package to be built with the uv build backend that uses all features exposed by the build backend
    Requires-Dist: anyio>=4,<5
    Requires-Python: >=3.12
    Description-Content-Type: text/markdown

    # built_by_uv

    A package to be built with the uv build backend that uses all features exposed by the build backend.
    "###);

    let record_file = metadata_dir
        .path()
        .join("built_by_uv-0.1.0.dist-info/RECORD");
    assert_snapshot!(fs_err::read_to_string(record_file).unwrap(), @r###"
    built_by_uv-0.1.0.dist-info/WHEEL,sha256=3da1bfa0e8fd1b6cc246aa0b2b44a35815596c600cb485c39a6f8c106c3d5a8d,83
    built_by_uv-0.1.0.dist-info/METADATA,sha256=acb91f5a18cb53fa57b45eb4590ea13195a774c856a9dd8cf27cc5435d6451b6,372
    built_by_uv-0.1.0.dist-info/RECORD,,
    "###);

    let wheel_file = metadata_dir
        .path()
        .join("built_by_uv-0.1.0.dist-info/WHEEL");
    assert_snapshot!(fs_err::read_to_string(wheel_file).unwrap(), @r###"
        Wheel-Version: 1.0
        Generator: uv 1.0.0+test
        Root-Is-Purelib: true
        Tag: py3-none-any
    "###);
}

/// Test that source tree -> source dist -> wheel includes the right files and is stable and
/// deterministic in dependent of the build path.
#[test]
fn built_by_uv_building() {
    let built_by_uv = Path::new("../../scripts/packages/built-by-uv");
    let src = TempDir::new().unwrap();
    for dir in ["src", "tests", "data-dir", "third-party-licenses"] {
        copy_dir_all(built_by_uv.join(dir), src.path().join(dir)).unwrap();
    }
    for dir in [
        "pyproject.toml",
        "README.md",
        "uv.lock",
        "LICENSE-APACHE",
        "LICENSE-MIT",
    ] {
        fs_err::copy(built_by_uv.join(dir), src.path().join(dir)).unwrap();
    }

    // Build a wheel from the source tree
    let direct_output_dir = TempDir::new().unwrap();
    build_wheel(
        src.path(),
        direct_output_dir.path(),
        None,
        WheelSettings::default(),
        "1.0.0+test",
    )
    .unwrap();

    let wheel = zip::ZipArchive::new(
        File::open(
            direct_output_dir
                .path()
                .join("built_by_uv-0.1.0-py3-none-any.whl"),
        )
        .unwrap(),
    )
    .unwrap();
    let mut direct_wheel_contents: Vec<_> = wheel.file_names().collect();
    direct_wheel_contents.sort_unstable();

    // Build a source dist from the source tree
    let source_dist_dir = TempDir::new().unwrap();
    build_source_dist(
        src.path(),
        source_dist_dir.path(),
        SourceDistSettings::default(),
        "1.0.0+test",
    )
    .unwrap();

    // Build a wheel from the source dist
    let sdist_tree = TempDir::new().unwrap();
    let source_dist_path = source_dist_dir.path().join("built_by_uv-0.1.0.tar.gz");
    let sdist_reader = BufReader::new(File::open(&source_dist_path).unwrap());
    let mut source_dist = tar::Archive::new(GzDecoder::new(sdist_reader));
    let mut source_dist_contents: Vec<_> = source_dist
        .entries()
        .unwrap()
        .map(|entry| entry.unwrap().path().unwrap().to_str().unwrap().to_string())
        .collect();
    source_dist_contents.sort();
    // Reset the reader and unpack
    let sdist_reader = BufReader::new(File::open(&source_dist_path).unwrap());
    let mut source_dist = tar::Archive::new(GzDecoder::new(sdist_reader));
    source_dist.unpack(sdist_tree.path()).unwrap();
    drop(source_dist_dir);

    let indirect_output_dir = TempDir::new().unwrap();
    build_wheel(
        &sdist_tree.path().join("built_by_uv-0.1.0"),
        indirect_output_dir.path(),
        None,
        WheelSettings::default(),
        "1.0.0+test",
    )
    .unwrap();

    // Check that we write deterministic wheels.
    let wheel_filename = "built_by_uv-0.1.0-py3-none-any.whl";
    assert_eq!(
        fs_err::read(direct_output_dir.path().join(wheel_filename)).unwrap(),
        fs_err::read(indirect_output_dir.path().join(wheel_filename)).unwrap()
    );

    // Check the contained files and directories
    assert_snapshot!(source_dist_contents.iter().map(|path| path.replace('\\', "/")).join("\n"), @r"
        built_by_uv-0.1.0/LICENSE-APACHE
        built_by_uv-0.1.0/LICENSE-MIT
        built_by_uv-0.1.0/PKG-INFO
        built_by_uv-0.1.0/README.md
        built_by_uv-0.1.0/pyproject.toml
        built_by_uv-0.1.0/src/built_by_uv
        built_by_uv-0.1.0/src/built_by_uv/__init__.py
        built_by_uv-0.1.0/src/built_by_uv/arithmetic
        built_by_uv-0.1.0/src/built_by_uv/arithmetic/__init__.py
        built_by_uv-0.1.0/src/built_by_uv/arithmetic/circle.py
        built_by_uv-0.1.0/src/built_by_uv/arithmetic/pi.txt
        built_by_uv-0.1.0/third-party-licenses/PEP-401.txt
    ");

    let wheel = zip::ZipArchive::new(
        File::open(
            indirect_output_dir
                .path()
                .join("built_by_uv-0.1.0-py3-none-any.whl"),
        )
        .unwrap(),
    )
    .unwrap();
    let mut indirect_wheel_contents: Vec<_> = wheel.file_names().collect();
    indirect_wheel_contents.sort_unstable();
    assert_eq!(indirect_wheel_contents, direct_wheel_contents);

    assert_snapshot!(indirect_wheel_contents.iter().map(|path| path.replace('\\', "/")).join("\n"), @r"
        built_by_uv-0.1.0.dist-info/
        built_by_uv-0.1.0.dist-info/METADATA
        built_by_uv-0.1.0.dist-info/RECORD
        built_by_uv-0.1.0.dist-info/WHEEL
        built_by_uv-0.1.0.dist-info/licenses/
        built_by_uv-0.1.0.dist-info/licenses/LICENSE-APACHE
        built_by_uv-0.1.0.dist-info/licenses/LICENSE-MIT
        built_by_uv-0.1.0.dist-info/licenses/third-party-licenses/PEP-401.txt
        built_by_uv/
        built_by_uv/__init__.py
        built_by_uv/arithmetic/
        built_by_uv/arithmetic/__init__.py
        built_by_uv/arithmetic/circle.py
        built_by_uv/arithmetic/pi.txt
    ");
}

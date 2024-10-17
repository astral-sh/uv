use super::*;

#[test]
fn err_not_whl_extension() {
    let err = WheelFilename::from_str("foo.rs").unwrap_err();
    insta::assert_snapshot!(err, @r###"The wheel filename "foo.rs" is invalid: Must end with .whl"###);
}

#[test]
fn err_1_part_empty() {
    let err = WheelFilename::from_str(".whl").unwrap_err();
    insta::assert_snapshot!(err, @r###"The wheel filename ".whl" is invalid: Must have a version"###);
}

#[test]
fn err_1_part_no_version() {
    let err = WheelFilename::from_str("foo.whl").unwrap_err();
    insta::assert_snapshot!(err, @r###"The wheel filename "foo.whl" is invalid: Must have a version"###);
}

#[test]
fn err_2_part_no_pythontag() {
    let err = WheelFilename::from_str("foo-version.whl").unwrap_err();
    insta::assert_snapshot!(err, @r###"The wheel filename "foo-version.whl" is invalid: Must have a Python tag"###);
}

#[test]
fn err_3_part_no_abitag() {
    let err = WheelFilename::from_str("foo-version-python.whl").unwrap_err();
    insta::assert_snapshot!(err, @r###"The wheel filename "foo-version-python.whl" is invalid: Must have an ABI tag"###);
}

#[test]
fn err_4_part_no_platformtag() {
    let err = WheelFilename::from_str("foo-version-python-abi.whl").unwrap_err();
    insta::assert_snapshot!(err, @r###"The wheel filename "foo-version-python-abi.whl" is invalid: Must have a platform tag"###);
}

#[test]
fn err_too_many_parts() {
    let err = WheelFilename::from_str("foo-1.2.3-build-python-abi-platform-oops.whl").unwrap_err();
    insta::assert_snapshot!(err, @r###"The wheel filename "foo-1.2.3-build-python-abi-platform-oops.whl" is invalid: Must have 5 or 6 components, but has more"###);
}

#[test]
fn err_invalid_package_name() {
    let err = WheelFilename::from_str("f!oo-1.2.3-python-abi-platform.whl").unwrap_err();
    insta::assert_snapshot!(err, @r###"The wheel filename "f!oo-1.2.3-python-abi-platform.whl" has an invalid package name"###);
}

#[test]
fn err_invalid_version() {
    let err = WheelFilename::from_str("foo-x.y.z-python-abi-platform.whl").unwrap_err();
    insta::assert_snapshot!(err, @r###"The wheel filename "foo-x.y.z-python-abi-platform.whl" has an invalid version: expected version to start with a number, but no leading ASCII digits were found"###);
}

#[test]
fn err_invalid_build_tag() {
    let err = WheelFilename::from_str("foo-1.2.3-tag-python-abi-platform.whl").unwrap_err();
    insta::assert_snapshot!(err, @r###"The wheel filename "foo-1.2.3-tag-python-abi-platform.whl" has an invalid build tag: must start with a digit"###);
}

#[test]
fn ok_single_tags() {
    insta::assert_debug_snapshot!(WheelFilename::from_str("foo-1.2.3-foo-bar-baz.whl"));
}

#[test]
fn ok_multiple_tags() {
    insta::assert_debug_snapshot!(WheelFilename::from_str(
        "foo-1.2.3-ab.cd.ef-gh-ij.kl.mn.op.qr.st.whl"
    ));
}

#[test]
fn ok_build_tag() {
    insta::assert_debug_snapshot!(WheelFilename::from_str(
        "foo-1.2.3-202206090410-python-abi-platform.whl"
    ));
}

#[test]
fn from_and_to_string() {
    let wheel_names = &[
        "django_allauth-0.51.0-py3-none-any.whl",
        "osm2geojson-0.2.4-py3-none-any.whl",
        "numpy-1.26.2-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl",
    ];
    for wheel_name in wheel_names {
        assert_eq!(
            WheelFilename::from_str(wheel_name).unwrap().to_string(),
            *wheel_name
        );
    }
}

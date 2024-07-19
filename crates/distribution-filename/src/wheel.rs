use std::fmt::{Display, Formatter};
use std::str::FromStr;

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;
use url::Url;

use pep440_rs::{Version, VersionParseError, VersionSpecifiers};
use platform_tags::{TagCompatibility, Tags};
use uv_normalize::{InvalidNameError, PackageName};

use crate::{BuildTag, BuildTagError};

#[derive(Debug, Clone, Eq, PartialEq, Hash, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
pub struct WheelFilename {
    pub name: PackageName,
    pub version: Version,
    pub build_tag: Option<BuildTag>,
    pub python_tag: Vec<String>,
    pub abi_tag: Vec<String>,
    pub platform_tag: Vec<String>,
}

impl FromStr for WheelFilename {
    type Err = WheelFilenameError;

    fn from_str(filename: &str) -> Result<Self, Self::Err> {
        let stem = filename.strip_suffix(".whl").ok_or_else(|| {
            WheelFilenameError::InvalidWheelFileName(
                filename.to_string(),
                "Must end with .whl".to_string(),
            )
        })?;
        Self::parse(stem, filename)
    }
}

impl Display for WheelFilename {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}-{}-{}.whl",
            self.name.as_dist_info_name(),
            self.version,
            self.get_tag()
        )
    }
}

impl WheelFilename {
    /// Returns `true` if the wheel is compatible with the given tags.
    pub fn is_compatible(&self, compatible_tags: &Tags) -> bool {
        compatible_tags.is_compatible(&self.python_tag, &self.abi_tag, &self.platform_tag)
    }

    /// Return the [`TagCompatibility`] of the wheel with the given tags
    pub fn compatibility(&self, compatible_tags: &Tags) -> TagCompatibility {
        compatible_tags.compatibility(&self.python_tag, &self.abi_tag, &self.platform_tag)
    }

    /// Returns `false` if the wheel's tags state it can't be used in the given Python version
    /// range.
    ///
    /// It is meant to filter out clearly unusable wheels with perfect specificity and acceptable
    /// sensitivity, we return `true` if the tags are unknown.
    pub fn matches_requires_python(&self, specifiers: &VersionSpecifiers) -> bool {
        self.abi_tag.iter().any(|abi_tag| {
            if abi_tag == "abi3" {
                // Universal tags are allowed.
                true
            } else if abi_tag == "none" {
                self.python_tag.iter().any(|python_tag| {
                    // Remove `py2-none-any` and `py27-none-any`.
                    if python_tag.starts_with("py2") {
                        return false;
                    }

                    // Remove (e.g.) `cp36-none-any` if the specifier is `==3.10.*`.
                    let Some(minor) = python_tag
                        .strip_prefix("cp3")
                        .or_else(|| python_tag.strip_prefix("pp3"))
                        .or_else(|| python_tag.strip_prefix("py3"))
                    else {
                        return true;
                    };
                    let Ok(minor) = minor.parse::<u64>() else {
                        return true;
                    };
                    let version = Version::new([3, minor]);
                    specifiers.contains(&version)
                })
            } else if abi_tag.starts_with("cp2") || abi_tag.starts_with("pypy2") {
                // Python 2 is never allowed.
                false
            } else if let Some(minor_no_dot_abi) = abi_tag.strip_prefix("cp3") {
                // Remove ABI tags, both old (dmu) and future (t, and all other letters).
                let minor_not_dot = minor_no_dot_abi.trim_matches(char::is_alphabetic);
                let Ok(minor) = minor_not_dot.parse::<u64>() else {
                    // Unknown version pattern are allowed.
                    return true;
                };

                let version = Version::new([3, minor]);
                specifiers.contains(&version)
            } else if let Some(minor_no_dot_abi) = abi_tag.strip_prefix("pypy3") {
                // Given  `pypy39_pp73`, we just removed `pypy3`, now we remove `_pp73` ...
                let Some((minor_not_dot, _)) = minor_no_dot_abi.split_once('_') else {
                    // Unknown version pattern are allowed.
                    return true;
                };
                // ... and get `9`.
                let Ok(minor) = minor_not_dot.parse::<u64>() else {
                    // Unknown version pattern are allowed.
                    return true;
                };

                let version = Version::new([3, minor]);
                specifiers.contains(&version)
            } else {
                // Unknown python tag -> allowed.
                true
            }
        })
    }

    /// The wheel filename without the extension.
    pub fn stem(&self) -> String {
        format!(
            "{}-{}-{}",
            self.name.as_dist_info_name(),
            self.version,
            self.get_tag()
        )
    }

    /// Parse a wheel filename from the stem (e.g., `foo-1.2.3-py3-none-any`).
    pub fn from_stem(stem: &str) -> Result<Self, WheelFilenameError> {
        Self::parse(stem, stem)
    }

    /// Get the tag for this wheel.
    fn get_tag(&self) -> String {
        format!(
            "{}-{}-{}",
            self.python_tag.join("."),
            self.abi_tag.join("."),
            self.platform_tag.join(".")
        )
    }

    /// Parse a wheel filename from the stem (e.g., `foo-1.2.3-py3-none-any`).
    ///
    /// The originating `filename` is used for high-fidelity error messages.
    fn parse(stem: &str, filename: &str) -> Result<Self, WheelFilenameError> {
        // The wheel filename should contain either five or six entries. If six, then the third
        // entry is the build tag. If five, then the third entry is the Python tag.
        // https://www.python.org/dev/peps/pep-0427/#file-name-convention
        //
        // 2023-11-08(burntsushi): It looks like the code below actually drops
        // the build tag if one is found. According to PEP 0427, the build tag
        // is used to break ties. This might mean that we generate identical
        // `WheelName` values for multiple distinct wheels, but it's not clear
        // if this is a problem in practice.
        let mut parts = stem.split('-');

        let name = parts
            .next()
            .expect("split always yields 1 or more elements");

        let Some(version) = parts.next() else {
            return Err(WheelFilenameError::InvalidWheelFileName(
                filename.to_string(),
                "Must have a version".to_string(),
            ));
        };

        let Some(build_tag_or_python_tag) = parts.next() else {
            return Err(WheelFilenameError::InvalidWheelFileName(
                filename.to_string(),
                "Must have a Python tag".to_string(),
            ));
        };

        let Some(python_tag_or_abi_tag) = parts.next() else {
            return Err(WheelFilenameError::InvalidWheelFileName(
                filename.to_string(),
                "Must have an ABI tag".to_string(),
            ));
        };

        let Some(abi_tag_or_platform_tag) = parts.next() else {
            return Err(WheelFilenameError::InvalidWheelFileName(
                filename.to_string(),
                "Must have a platform tag".to_string(),
            ));
        };

        let (name, version, build_tag, python_tag, abi_tag, platform_tag) =
            if let Some(platform_tag) = parts.next() {
                if parts.next().is_some() {
                    return Err(WheelFilenameError::InvalidWheelFileName(
                        filename.to_string(),
                        "Must have 5 or 6 components, but has more".to_string(),
                    ));
                }
                (
                    name,
                    version,
                    Some(build_tag_or_python_tag),
                    python_tag_or_abi_tag,
                    abi_tag_or_platform_tag,
                    platform_tag,
                )
            } else {
                (
                    name,
                    version,
                    None,
                    build_tag_or_python_tag,
                    python_tag_or_abi_tag,
                    abi_tag_or_platform_tag,
                )
            };

        let name = PackageName::from_str(name)
            .map_err(|err| WheelFilenameError::InvalidPackageName(filename.to_string(), err))?;
        let version = Version::from_str(version)
            .map_err(|err| WheelFilenameError::InvalidVersion(filename.to_string(), err))?;
        let build_tag = build_tag
            .map(|build_tag| {
                BuildTag::from_str(build_tag)
                    .map_err(|err| WheelFilenameError::InvalidBuildTag(filename.to_string(), err))
            })
            .transpose()?;
        Ok(Self {
            name,
            version,
            build_tag,
            python_tag: python_tag.split('.').map(String::from).collect(),
            abi_tag: abi_tag.split('.').map(String::from).collect(),
            platform_tag: platform_tag.split('.').map(String::from).collect(),
        })
    }
}

impl TryFrom<&Url> for WheelFilename {
    type Error = WheelFilenameError;

    fn try_from(url: &Url) -> Result<Self, Self::Error> {
        let filename = url
            .path_segments()
            .ok_or_else(|| {
                WheelFilenameError::InvalidWheelFileName(
                    url.to_string(),
                    "URL must have a path".to_string(),
                )
            })?
            .last()
            .ok_or_else(|| {
                WheelFilenameError::InvalidWheelFileName(
                    url.to_string(),
                    "URL must contain a filename".to_string(),
                )
            })?;
        Self::from_str(filename)
    }
}

impl<'de> Deserialize<'de> for WheelFilename {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        FromStr::from_str(&s).map_err(de::Error::custom)
    }
}

impl Serialize for WheelFilename {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Error, Debug)]
pub enum WheelFilenameError {
    #[error("The wheel filename \"{0}\" is invalid: {1}")]
    InvalidWheelFileName(String, String),
    #[error("The wheel filename \"{0}\" has an invalid version: {1}")]
    InvalidVersion(String, VersionParseError),
    #[error("The wheel filename \"{0}\" has an invalid package name")]
    InvalidPackageName(String, InvalidNameError),
    #[error("The wheel filename \"{0}\" has an invalid build tag: {1}")]
    InvalidBuildTag(String, BuildTagError),
}

#[cfg(test)]
mod tests {
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
        let err =
            WheelFilename::from_str("foo-1.2.3-build-python-abi-platform-oops.whl").unwrap_err();
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

    #[test]
    fn test_requires_python_included() {
        let version_specifiers = VersionSpecifiers::from_str("==3.10.*").unwrap();
        let wheel_names = &[
            "bcrypt-4.1.3-cp37-abi3-macosx_10_12_universal2.whl",
            "black-24.4.2-cp310-cp310-win_amd64.whl",
            "black-24.4.2-cp310-none-win_amd64.whl",
            "cbor2-5.6.4-py3-none-any.whl",
            "watchfiles-0.22.0-pp310-pypy310_pp73-macosx_11_0_arm64.whl",
        ];
        for wheel_name in wheel_names {
            assert!(
                WheelFilename::from_str(wheel_name)
                    .unwrap()
                    .matches_requires_python(&version_specifiers),
                "{wheel_name}"
            );
        }
    }

    #[test]
    fn test_requires_python_dropped() {
        let version_specifiers = VersionSpecifiers::from_str("==3.10.*").unwrap();
        let wheel_names = &[
            "PySocks-1.7.1-py27-none-any.whl",
            "black-24.4.2-cp39-cp39-win_amd64.whl",
            "psutil-6.0.0-cp36-cp36m-win32.whl",
            "pydantic_core-2.20.1-pp39-pypy39_pp73-win_amd64.whl",
            "torch-1.10.0-cp36-none-macosx_10_9_x86_64.whl",
            "torch-1.10.0-py36-none-macosx_10_9_x86_64.whl",
        ];
        for wheel_name in wheel_names {
            assert!(
                !WheelFilename::from_str(wheel_name)
                    .unwrap()
                    .matches_requires_python(&version_specifiers),
                "{wheel_name}"
            );
        }
    }
}

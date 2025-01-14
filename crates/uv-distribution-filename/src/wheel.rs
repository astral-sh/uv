use std::fmt::{Display, Formatter};

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::str::FromStr;
use thiserror::Error;
use url::Url;

use uv_normalize::{InvalidNameError, PackageName};
use uv_pep440::{Version, VersionParseError};
use uv_platform_tags::{
    AbiTag, LanguageTag, ParseAbiTagError, ParseLanguageTagError, ParsePlatformTagError,
    PlatformTag, TagCompatibility, Tags,
};

use crate::{BuildTag, BuildTagError};
use crate::split::MemchrSplitter;

#[derive(
    Debug,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[rkyv(derive(Debug))]
pub struct WheelFilename {
    pub name: PackageName,
    pub version: Version,
    tags: WheelTag,
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
            self.tags,
        )
    }
}

impl WheelFilename {
    /// Create a [`WheelFilename`] from its components.
    pub fn new(
        name: PackageName,
        version: Version,
        python_tag: LanguageTag,
        abi_tag: AbiTag,
        platform_tag: PlatformTag,
    ) -> Self {
        Self {
            name,
            version,
            tags: WheelTag::Small {
                small: WheelTagSmall {
                    python_tag,
                    abi_tag,
                    platform_tag,
                },
            },
        }
    }

    /// Returns `true` if the wheel is compatible with the given tags.
    pub fn is_compatible(&self, compatible_tags: &Tags) -> bool {
        compatible_tags.is_compatible(self.python_tags(), self.abi_tags(), self.platform_tags())
    }

    /// Return the [`TagCompatibility`] of the wheel with the given tags
    pub fn compatibility(&self, compatible_tags: &Tags) -> TagCompatibility {
        compatible_tags.compatibility(self.python_tags(), self.abi_tags(), self.platform_tags())
    }

    /// The wheel filename without the extension.
    pub fn stem(&self) -> String {
        format!(
            "{}-{}-{}",
            self.name.as_dist_info_name(),
            self.version,
            self.tags
        )
    }

    /// Return the wheel's Python tags.
    pub fn python_tags(&self) -> &[LanguageTag] {
        match &self.tags {
            WheelTag::Small { small } => std::slice::from_ref(&small.python_tag),
            WheelTag::Large { large } => large.python_tag.as_slice(),
        }
    }

    /// Return the wheel's ABI tags.
    pub fn abi_tags(&self) -> &[AbiTag] {
        match &self.tags {
            WheelTag::Small { small } => std::slice::from_ref(&small.abi_tag),
            WheelTag::Large { large } => large.abi_tag.as_slice(),
        }
    }

    /// Return the wheel's platform tags.
    pub fn platform_tags(&self) -> &[PlatformTag] {
        match &self.tags {
            WheelTag::Small { small } => std::slice::from_ref(&small.platform_tag),
            WheelTag::Large { large } => large.platform_tag.as_slice(),
        }
    }

    /// Return the wheel's build tag, if present.
    pub fn build_tag(&self) -> Option<&BuildTag> {
        match &self.tags {
            WheelTag::Small { .. } => None,
            WheelTag::Large { large } => large.build_tag.as_ref(),
        }
    }

    /// Parse a wheel filename from the stem (e.g., `foo-1.2.3-py3-none-any`).
    pub fn from_stem(stem: &str) -> Result<Self, WheelFilenameError> {
        Self::parse(stem, stem)
    }

    /// Parse a wheel filename from the stem (e.g., `foo-1.2.3-py3-none-any`).
    pub fn fast_from_str(filename: &str) -> Result<Self, WheelFilenameError> {
        let stem = filename.strip_suffix(".whl").ok_or_else(|| {
            WheelFilenameError::InvalidWheelFileName(
                filename.to_string(),
                "Must end with .whl".to_string(),
            )
        })?;
        Self::fast_parse(stem, filename)
    }

    /// Parse a wheel filename from the stem (e.g., `foo-1.2.3-py3-none-any`).
    ///
    /// The originating `filename` is used for high-fidelity error messages.
    fn parse(stem: &str, filename: &str) -> Result<Self, WheelFilenameError> {
        // The wheel filename should contain either five or six entries. If six, then the third
        // entry is the build tag. If five, then the third entry is the Python tag.
        // https://www.python.org/dev/peps/pep-0427/#file-name-convention
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

        let tags = if build_tag.is_some()
            || python_tag.contains('.')
            || abi_tag.contains('.')
            || platform_tag.contains('.')
        {
            WheelTag::Large {
                large: Box::new(WheelTagLarge {
                    build_tag,
                    python_tag: python_tag
                        .split('.')
                        .map(LanguageTag::from_str)
                        .collect::<Result<_, _>>()
                        .map_err(|err| {
                            WheelFilenameError::InvalidLanguageTag(filename.to_string(), err)
                        })?,
                    abi_tag: abi_tag
                        .split('.')
                        .map(AbiTag::from_str)
                        .collect::<Result<_, _>>()
                        .map_err(|err| {
                            WheelFilenameError::InvalidAbiTag(filename.to_string(), err)
                        })?,
                    platform_tag: platform_tag
                        .split('.')
                        .map(PlatformTag::from_str)
                        .collect::<Result<_, _>>()
                        .map_err(|err| {
                            WheelFilenameError::InvalidPlatformTag(filename.to_string(), err)
                        })?,
                }),
            }
        } else {
            WheelTag::Small {
                small: WheelTagSmall {
                    python_tag: LanguageTag::from_str(python_tag).map_err(|err| {
                        WheelFilenameError::InvalidLanguageTag(filename.to_string(), err)
                    })?,
                    abi_tag: AbiTag::from_str(abi_tag).map_err(|err| {
                        WheelFilenameError::InvalidAbiTag(filename.to_string(), err)
                    })?,
                    platform_tag: PlatformTag::from_str(platform_tag).map_err(|err| {
                        WheelFilenameError::InvalidPlatformTag(filename.to_string(), err)
                    })?,
                },
            }
        };

        Ok(Self {
            name,
            version,
            tags,
        })
    }

    /// Parse a wheel filename from the stem (e.g., `foo-1.2.3-py3-none-any`).
    ///
    /// The originating `filename` is used for high-fidelity error messages.
    fn fast_parse(stem: &str, filename: &str) -> Result<Self, WheelFilenameError> {
        // The wheel filename should contain either five or six entries. If six, then the third
        // entry is the build tag. If five, then the third entry is the Python tag.
        // https://www.python.org/dev/peps/pep-0427/#file-name-convention
        let mut indexes = memchr::Memchr::new(b'-', stem.as_bytes());

        let Some(next) = indexes.next() else {
            return Err(WheelFilenameError::InvalidWheelFileName(
                filename.to_string(),
                "Must have a version".to_string(),
            ));
        };
        let name = &stem[..next];
        let prev = next;

        let Some(next) = indexes.next() else {
            return Err(WheelFilenameError::InvalidWheelFileName(
                filename.to_string(),
                "Must have a Python tag".to_string(),
            ));
        };
        let version = &stem[prev + 1..next];
        let prev = next;

        let Some(next) = indexes.next() else {
            return Err(WheelFilenameError::InvalidWheelFileName(
                filename.to_string(),
                "Must have an ABI tag".to_string(),
            ));
        };
        let build_tag_or_python_tag = &stem[prev + 1..next];
        let prev = next;

        let Some(next) = indexes.next() else {
            return Err(WheelFilenameError::InvalidWheelFileName(
                filename.to_string(),
                "Must have a platform tag".to_string(),
            ));
        };
        let python_tag_or_abi_tag = &stem[prev + 1..next];
        let prev = next;

        let (name, version, build_tag, python_tag, abi_tag, platform_tag) =
            if let Some(next) = indexes.next() {
                if indexes.next().is_some() {
                    return Err(WheelFilenameError::InvalidWheelFileName(
                        filename.to_string(),
                        "Must have 5 or 6 components, but has more".to_string(),
                    ));
                }
                let abi_tag = &stem[prev + 1..next];
                let platform_tag = &stem[next + 1..];
                (
                    name,
                    version,
                    Some(build_tag_or_python_tag),
                    python_tag_or_abi_tag,
                    abi_tag,
                    platform_tag,
                )
            } else {
                let platform_tag = &stem[prev + 1..];
                (
                    name,
                    version,
                    None,
                    build_tag_or_python_tag,
                    python_tag_or_abi_tag,
                    platform_tag
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

        let tags = if build_tag.is_some()
            || python_tag.contains('.')
            || abi_tag.contains('.')
            || platform_tag.contains('.')
        {
            WheelTag::Large {
                large: Box::new(WheelTagLarge {
                    build_tag,
                    python_tag: python_tag
                        .split('.')
                        .map(LanguageTag::from_str)
                        .collect::<Result<_, _>>()
                        .map_err(|err| {
                            WheelFilenameError::InvalidLanguageTag(filename.to_string(), err)
                        })?,
                    abi_tag: abi_tag
                        .split('.')
                        .map(AbiTag::from_str)
                        .collect::<Result<_, _>>()
                        .map_err(|err| {
                            WheelFilenameError::InvalidAbiTag(filename.to_string(), err)
                        })?,
                    platform_tag: platform_tag
                        .split('.')
                        .map(PlatformTag::from_str)
                        .collect::<Result<_, _>>()
                        .map_err(|err| {
                            WheelFilenameError::InvalidPlatformTag(filename.to_string(), err)
                        })?,
                }),
            }
        } else {
            WheelTag::Small {
                small: WheelTagSmall {
                    python_tag: LanguageTag::from_str(python_tag).map_err(|err| {
                        WheelFilenameError::InvalidLanguageTag(filename.to_string(), err)
                    })?,
                    abi_tag: AbiTag::from_str(abi_tag).map_err(|err| {
                        WheelFilenameError::InvalidAbiTag(filename.to_string(), err)
                    })?,
                    platform_tag: PlatformTag::from_str(platform_tag).map_err(|err| {
                        WheelFilenameError::InvalidPlatformTag(filename.to_string(), err)
                    })?,
                },
            }
        };

        Ok(Self {
            name,
            version,
            tags,
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

/// A [`SmallVec`] type for storing tags.
///
/// Wheels tend to include a single language, ABI, and platform tag, so we use a [`SmallVec`] with a
/// capacity of 1 to optimize for this common case.
pub type TagSet<T> = smallvec::SmallVec<[T; 3]>;

/// The portion of the wheel filename following the name and version: the optional build tag, along
/// with the Python tag(s), ABI tag(s), and platform tag(s).
///
/// Most wheels consist of a single Python, ABI, and platform tag (and no build tag). We represent
/// such wheels with [`WheelTagSmall`], a variant with a smaller memory footprint and (generally)
/// zero allocations. The [`WheelTagLarge`] variant is used for wheels with multiple tags and/or a
/// build tag.
#[derive(
    Debug,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[rkyv(derive(Debug))]
enum WheelTag {
    Small { small: WheelTagSmall },
    Large { large: Box<WheelTagLarge> },
}

impl Display for WheelTag {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Small { small } => write!(f, "{small}"),
            Self::Large { large } => write!(f, "{large}"),
        }
    }
}

#[derive(
    Debug,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[rkyv(derive(Debug))]
#[allow(clippy::struct_field_names)]
struct WheelTagSmall {
    python_tag: LanguageTag,
    abi_tag: AbiTag,
    platform_tag: PlatformTag,
}

impl Display for WheelTagSmall {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}-{}-{}",
            self.python_tag, self.abi_tag, self.platform_tag
        )
    }
}

#[derive(
    Debug,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[rkyv(derive(Debug))]
#[allow(clippy::struct_field_names)]
pub struct WheelTagLarge {
    build_tag: Option<BuildTag>,
    python_tag: TagSet<LanguageTag>,
    abi_tag: TagSet<AbiTag>,
    platform_tag: TagSet<PlatformTag>,
}

impl Display for WheelTagLarge {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let Some(build_tag) = &self.build_tag {
            write!(f, "{build_tag}-")?;
        }
        write!(
            f,
            "{}-{}-{}",
            self.python_tag
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("."),
            self.abi_tag
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("."),
            self.platform_tag
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("."),
        )
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
    #[error("The wheel filename \"{0}\" has an invalid language tag: {1}")]
    InvalidLanguageTag(String, ParseLanguageTagError),
    #[error("The wheel filename \"{0}\" has an invalid ABI tag: {1}")]
    InvalidAbiTag(String, ParseAbiTagError),
    #[error("The wheel filename \"{0}\" has an invalid platform tag: {1}")]
    InvalidPlatformTag(String, ParsePlatformTagError),
    #[error("The wheel filename \"{0}\" is missing a language tag")]
    MissingLanguageTag(String),
    #[error("The wheel filename \"{0}\" is missing an ABI tag")]
    MissingAbiTag(String),
    #[error("The wheel filename \"{0}\" is missing a platform tag")]
    MissingPlatformTag(String),
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
        let err = WheelFilename::from_str("foo-1.2.3.whl").unwrap_err();
        insta::assert_snapshot!(err, @r###"The wheel filename "foo-1.2.3.whl" is invalid: Must have a Python tag"###);
    }

    #[test]
    fn err_3_part_no_abitag() {
        let err = WheelFilename::from_str("foo-1.2.3-py3.whl").unwrap_err();
        insta::assert_snapshot!(err, @r###"The wheel filename "foo-1.2.3-py3.whl" is invalid: Must have an ABI tag"###);
    }

    #[test]
    fn err_4_part_no_platformtag() {
        let err = WheelFilename::from_str("foo-1.2.3-py3-none.whl").unwrap_err();
        insta::assert_snapshot!(err, @r###"The wheel filename "foo-1.2.3-py3-none.whl" is invalid: Must have a platform tag"###);
    }

    #[test]
    fn err_too_many_parts() {
        let err =
            WheelFilename::from_str("foo-1.2.3-202206090410-py3-none-any-whoops.whl").unwrap_err();
        insta::assert_snapshot!(err, @r###"The wheel filename "foo-1.2.3-202206090410-py3-none-any-whoops.whl" is invalid: Must have 5 or 6 components, but has more"###);
    }

    #[test]
    fn err_invalid_package_name() {
        let err = WheelFilename::from_str("f!oo-1.2.3-py3-none-any.whl").unwrap_err();
        insta::assert_snapshot!(err, @r###"The wheel filename "f!oo-1.2.3-py3-none-any.whl" has an invalid package name"###);
    }

    #[test]
    fn err_invalid_version() {
        let err = WheelFilename::from_str("foo-x.y.z-py3-none-any.whl").unwrap_err();
        insta::assert_snapshot!(err, @r###"The wheel filename "foo-x.y.z-py3-none-any.whl" has an invalid version: expected version to start with a number, but no leading ASCII digits were found"###);
    }

    #[test]
    fn err_invalid_build_tag() {
        let err = WheelFilename::from_str("foo-1.2.3-tag-py3-none-any.whl").unwrap_err();
        insta::assert_snapshot!(err, @r###"The wheel filename "foo-1.2.3-tag-py3-none-any.whl" has an invalid build tag: must start with a digit"###);
    }

    #[test]
    fn ok_single_tags() {
        insta::assert_debug_snapshot!(WheelFilename::from_str("foo-1.2.3-py3-none-any.whl"));
    }

    #[test]
    fn ok_multiple_tags() {
        insta::assert_debug_snapshot!(WheelFilename::from_str(
            "foo-1.2.3-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl"
        ));
    }

    #[test]
    fn ok_build_tag() {
        insta::assert_debug_snapshot!(WheelFilename::from_str(
            "foo-1.2.3-202206090410-py3-none-any.whl"
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
}

use std::fmt::{Display, Formatter};
use std::hash::Hash;
use std::str::FromStr;

use memchr::memchr;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;
use url::Url;

use uv_cache_key::cache_digest;
use uv_normalize::{InvalidNameError, PackageName};
use uv_pep440::{Version, VersionParseError};
use uv_platform_tags::{
    AbiTag, LanguageTag, ParseAbiTagError, ParseLanguageTagError, ParsePlatformTagError,
    PlatformTag, TagCompatibility, Tags,
};

use crate::splitter::MemchrSplitter;
use crate::wheel_tag::{WheelTag, WheelTagLarge, WheelTagSmall};
use crate::{BuildTag, BuildTagError};

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

    /// Returns a consistent cache key with a maximum length of 64 characters.
    ///
    /// Prefers `{version}-{tags}` if such an identifier fits within the maximum allowed length;
    /// otherwise, uses a truncated version of the version and a digest of the tags.
    pub fn cache_key(&self) -> String {
        const CACHE_KEY_MAX_LEN: usize = 64;

        let full = format!("{}-{}", self.version, self.tags);
        if full.len() <= CACHE_KEY_MAX_LEN {
            return full;
        }

        // Create a digest of the tag string (instead of its individual fields) to retain
        // compatibility across platforms, Rust versions, etc.
        let digest = cache_digest(&format!("{}", self.tags));

        // Truncate the version, but avoid trailing dots, plus signs, etc. to avoid ambiguity.
        let version_width = CACHE_KEY_MAX_LEN - 1 /* dash */ - 16 /* digest */;
        let mut version = self.version.to_string();

        // PANIC SAFETY: version strings can only contain ASCII characters.
        version.truncate(version_width);
        let version = version.trim_end_matches(['.', '+']);

        format!("{version}-{digest}")
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
        // The wheel stem should not contain the `.whl` extension.
        if std::path::Path::new(stem)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("whl"))
        {
            return Err(WheelFilenameError::UnexpectedExtension(stem.to_string()));
        }
        Self::parse(stem, stem)
    }

    /// Parse a wheel filename from the stem (e.g., `foo-1.2.3-py3-none-any`).
    ///
    /// The originating `filename` is used for high-fidelity error messages.
    fn parse(stem: &str, filename: &str) -> Result<Self, WheelFilenameError> {
        // The wheel filename should contain either five or six entries. If six, then the third
        // entry is the build tag. If five, then the third entry is the Python tag.
        // https://www.python.org/dev/peps/pep-0427/#file-name-convention
        let mut splitter = memchr::Memchr::new(b'-', stem.as_bytes());

        let Some(version) = splitter.next() else {
            return Err(WheelFilenameError::InvalidWheelFileName(
                filename.to_string(),
                "Must have a version".to_string(),
            ));
        };

        let Some(build_tag_or_python_tag) = splitter.next() else {
            return Err(WheelFilenameError::InvalidWheelFileName(
                filename.to_string(),
                "Must have a Python tag".to_string(),
            ));
        };

        let Some(python_tag_or_abi_tag) = splitter.next() else {
            return Err(WheelFilenameError::InvalidWheelFileName(
                filename.to_string(),
                "Must have an ABI tag".to_string(),
            ));
        };

        let Some(abi_tag_or_platform_tag) = splitter.next() else {
            return Err(WheelFilenameError::InvalidWheelFileName(
                filename.to_string(),
                "Must have a platform tag".to_string(),
            ));
        };

        let (name, version, build_tag, python_tag, abi_tag, platform_tag, is_small) =
            if let Some(platform_tag) = splitter.next() {
                if splitter.next().is_some() {
                    return Err(WheelFilenameError::InvalidWheelFileName(
                        filename.to_string(),
                        "Must have 5 or 6 components, but has more".to_string(),
                    ));
                }
                (
                    &stem[..version],
                    &stem[version + 1..build_tag_or_python_tag],
                    Some(&stem[build_tag_or_python_tag + 1..python_tag_or_abi_tag]),
                    &stem[python_tag_or_abi_tag + 1..abi_tag_or_platform_tag],
                    &stem[abi_tag_or_platform_tag + 1..platform_tag],
                    &stem[platform_tag + 1..],
                    // Always take the slow path if a build tag is present.
                    false,
                )
            } else {
                (
                    &stem[..version],
                    &stem[version + 1..build_tag_or_python_tag],
                    None,
                    &stem[build_tag_or_python_tag + 1..python_tag_or_abi_tag],
                    &stem[python_tag_or_abi_tag + 1..abi_tag_or_platform_tag],
                    &stem[abi_tag_or_platform_tag + 1..],
                    // Determine whether any of the tag types contain a period, which would indicate
                    // that at least one of the tag types includes multiple tags (which in turn
                    // necessitates taking the slow path).
                    memchr(b'.', stem[build_tag_or_python_tag..].as_bytes()).is_none(),
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

        let tags = if let Some(small) = is_small
            .then(|| {
                Some(WheelTagSmall {
                    python_tag: LanguageTag::from_str(python_tag).ok()?,
                    abi_tag: AbiTag::from_str(abi_tag).ok()?,
                    platform_tag: PlatformTag::from_str(platform_tag).ok()?,
                })
            })
            .flatten()
        {
            WheelTag::Small { small }
        } else {
            // Store the plaintext representation of the tags.
            let repr = &stem[build_tag_or_python_tag + 1..];
            WheelTag::Large {
                large: Box::new(WheelTagLarge {
                    build_tag,
                    python_tag: MemchrSplitter::split(python_tag, b'.')
                        .map(LanguageTag::from_str)
                        .filter_map(Result::ok)
                        .collect(),
                    abi_tag: MemchrSplitter::split(abi_tag, b'.')
                        .map(AbiTag::from_str)
                        .filter_map(Result::ok)
                        .collect(),
                    platform_tag: MemchrSplitter::split(platform_tag, b'.')
                        .map(PlatformTag::from_str)
                        .filter_map(Result::ok)
                        .collect(),
                    repr: repr.into(),
                }),
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
        struct Visitor;

        impl de::Visitor<'_> for Visitor {
            type Value = WheelFilename;

            fn expecting(&self, f: &mut Formatter) -> std::fmt::Result {
                f.write_str("a string")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                WheelFilename::from_str(v).map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_str(Visitor)
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
    #[error("The wheel stem \"{0}\" has an unexpected extension")]
    UnexpectedExtension(String),
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

    #[test]
    fn cache_key() {
        // Short names should use `version-tags` format.
        let filename = WheelFilename::from_str("django_allauth-0.51.0-py3-none-any.whl").unwrap();
        insta::assert_snapshot!(filename.cache_key(), @"0.51.0-py3-none-any");

        // Common `manylinux` names should use still use the `version-tags` format.
        let filename = WheelFilename::from_str(
            "numpy-1.26.2-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl",
        )
        .unwrap();
        insta::assert_snapshot!(filename.cache_key(), @"1.26.2-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64");

        // But larger names should use the `truncated(version)-digest(tags)` format.
        let filename = WheelFilename::from_str(
            "numpy-1.26.2-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.musllinux_1_2.whl",
        )
        .unwrap();
        insta::assert_snapshot!(filename.cache_key(), @"1.26.2-5a2adc379b2dc214");

        // Larger versions should get truncated.
        let filename = WheelFilename::from_str(
            "example-1.2.3.4.5.6.7.8.9.0.1.2.3.4.5.6.7.8.9.0.1.2.1.2.3.4.5.6.7.8.9.0.1.1.2-cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64.whl"
        ).unwrap();
        insta::assert_snapshot!(filename.cache_key(), @"1.2.3.4.5.6.7.8.9.0.1.2.3.4.5.6.7.8.9.0.1.2.1.2-80bf8598e9647cf7");
    }
}

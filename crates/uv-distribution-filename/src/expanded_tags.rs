use std::str::FromStr;

use memchr::memchr;
use thiserror::Error;

use uv_platform_tags::{
    AbiTag, LanguageTag, ParseAbiTagError, ParseLanguageTagError, ParsePlatformTagError,
    PlatformTag, TagCompatibility, Tags,
};

use crate::splitter::MemchrSplitter;
use crate::wheel_tag::{WheelTag, WheelTagLarge, WheelTagSmall};
use crate::{InvalidVariantLabel, VariantLabel};

/// The expanded wheel tags as stored in a `WHEEL` file.
///
/// For example, if a wheel filename included `py2.py3-none-any`, the `WHEEL` file would include:
/// ```
/// Tag: py2-none-any
/// Tag: py3-none-any
/// ```
///
/// This type stores those expanded tags.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct ExpandedTags(smallvec::SmallVec<[WheelTag; 1]>);

impl ExpandedTags {
    /// Parse a list of expanded wheel tags (e.g., `py3-none-any`).
    pub fn parse<'a>(tags: impl IntoIterator<Item = &'a str>) -> Result<Self, ExpandedTagError> {
        let tags = tags
            .into_iter()
            .map(parse_expanded_tag)
            .collect::<Result<_, _>>()?;
        Ok(Self(tags))
    }

    /// Returns `true` if the wheel is compatible with the given tags.
    pub fn is_compatible(&self, compatible_tags: &Tags) -> bool {
        self.0.iter().any(|tag| {
            compatible_tags.is_compatible(tag.python_tags(), tag.abi_tags(), tag.platform_tags())
        })
    }

    /// Return the Python tags in this expanded tag set.
    pub fn python_tags(&self) -> impl Iterator<Item = &LanguageTag> {
        self.0.iter().flat_map(WheelTag::python_tags)
    }

    /// Return the ABI tags in this expanded tag set.
    pub fn abi_tags(&self) -> impl Iterator<Item = &AbiTag> {
        self.0.iter().flat_map(WheelTag::abi_tags)
    }

    /// Return the platform tags in this expanded tag set.
    pub fn platform_tags(&self) -> impl Iterator<Item = &PlatformTag> {
        self.0.iter().flat_map(WheelTag::platform_tags)
    }

    /// Return the [`TagCompatibility`] of the wheel with the given tags
    pub fn compatibility(&self, compatible_tags: &Tags) -> TagCompatibility {
        compatible_tags.compatibility(
            self.python_tags().copied().collect::<Vec<_>>().as_slice(),
            self.abi_tags().copied().collect::<Vec<_>>().as_slice(),
            self.platform_tags().cloned().collect::<Vec<_>>().as_slice(),
        )
    }
}

#[derive(Error, Debug)]
pub enum ExpandedTagError {
    #[error("The wheel tag \"{0}\" is missing a language tag")]
    MissingLanguageTag(String),
    #[error("The wheel tag \"{0}\" is missing an ABI tag")]
    MissingAbiTag(String),
    #[error("The wheel tag \"{0}\" is missing a platform tag")]
    MissingPlatformTag(String),
    #[error("The wheel tag \"{0}\" contains too many segments")]
    ExtraSegment(String),
    #[error("The wheel tag \"{0}\" contains an invalid language tag")]
    InvalidLanguageTag(String, #[source] ParseLanguageTagError),
    #[error("The wheel tag \"{0}\" contains an invalid ABI tag")]
    InvalidAbiTag(String, #[source] ParseAbiTagError),
    #[error("The wheel tag \"{0}\" contains an invalid platform tag")]
    InvalidPlatformTag(String, #[source] ParsePlatformTagError),
    #[error("The wheel tag \"{0}\" contains an invalid variant label")]
    InvalidVariantLabel(String, #[source] InvalidVariantLabel),
}

/// Parse an expanded (i.e., simplified) wheel tag, e.g. `py3-none-any`.
///
/// Unlike parsing tags in a wheel filename, each tag in this case is expected to contain exactly
/// three segments separated by `-`: a language tag, an ABI tag, and a platform tag; however,
/// empirically, some build backends do emit multipart tags (like `cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64`),
/// so we allow those too.
fn parse_expanded_tag(tag: &str) -> Result<WheelTag, ExpandedTagError> {
    let mut splitter = memchr::Memchr::new(b'-', tag.as_bytes());
    if tag.is_empty() {
        return Err(ExpandedTagError::MissingLanguageTag(tag.to_string()));
    }
    let Some(python_tag_index) = splitter.next() else {
        return Err(ExpandedTagError::MissingAbiTag(tag.to_string()));
    };
    let Some(abi_tag_index) = splitter.next() else {
        return Err(ExpandedTagError::MissingPlatformTag(tag.to_string()));
    };
    let variant = splitter.next();
    if splitter.next().is_some() {
        return Err(ExpandedTagError::ExtraSegment(tag.to_string()));
    }

    let python_tag = &tag[..python_tag_index];
    let abi_tag = &tag[python_tag_index + 1..abi_tag_index];
    let platform_tag = &tag[abi_tag_index + 1..variant.unwrap_or(tag.len())];
    let variant = variant.map(|variant| &tag[variant + 1..]);

    let is_small = memchr(b'.', tag.as_bytes()).is_none();

    if let Some(small) = is_small
        .then(|| {
            Some(WheelTagSmall {
                python_tag: LanguageTag::from_str(python_tag).ok()?,
                abi_tag: AbiTag::from_str(abi_tag).ok()?,
                platform_tag: PlatformTag::from_str(platform_tag).ok()?,
            })
        })
        .flatten()
    {
        Ok(WheelTag::Small { small })
    } else {
        Ok(WheelTag::Large {
            large: Box::new(WheelTagLarge {
                build_tag: None,
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
                variant: variant
                    .map(VariantLabel::from_str)
                    .transpose()
                    .map_err(|err| ExpandedTagError::InvalidVariantLabel(tag.to_string(), err))?,
                repr: tag.into(),
            }),
        })
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_parse_simple_expanded_tag() {
        let tags = ExpandedTags::parse(vec!["py3-none-any"]).unwrap();

        insta::assert_debug_snapshot!(tags, @r"
        ExpandedTags(
            [
                Small {
                    small: WheelTagSmall {
                        python_tag: Python {
                            major: 3,
                            minor: None,
                        },
                        abi_tag: None,
                        platform_tag: Any,
                    },
                },
            ],
        )
        ");
    }

    #[test]
    fn test_parse_multiple_expanded_tags() {
        let tags = ExpandedTags::parse(vec![
            "py2-none-any",
            "py3-none-any",
            "cp39-cp39-linux_x86_64",
        ])
        .unwrap();

        insta::assert_debug_snapshot!(tags, @r"
        ExpandedTags(
            [
                Small {
                    small: WheelTagSmall {
                        python_tag: Python {
                            major: 2,
                            minor: None,
                        },
                        abi_tag: None,
                        platform_tag: Any,
                    },
                },
                Small {
                    small: WheelTagSmall {
                        python_tag: Python {
                            major: 3,
                            minor: None,
                        },
                        abi_tag: None,
                        platform_tag: Any,
                    },
                },
                Small {
                    small: WheelTagSmall {
                        python_tag: CPython {
                            python_version: (
                                3,
                                9,
                            ),
                        },
                        abi_tag: CPython {
                            gil_disabled: false,
                            python_version: (
                                3,
                                9,
                            ),
                        },
                        platform_tag: Linux {
                            arch: X86_64,
                        },
                    },
                },
            ],
        )
        ");
    }

    #[test]
    fn test_parse_complex_platform_tag() {
        let tags = ExpandedTags::parse(vec![
            "cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64",
        ])
        .unwrap();

        insta::assert_debug_snapshot!(tags, @r#"
        ExpandedTags(
            [
                Large {
                    large: WheelTagLarge {
                        build_tag: None,
                        python_tag: [
                            CPython {
                                python_version: (
                                    3,
                                    12,
                                ),
                            },
                        ],
                        abi_tag: [
                            CPython {
                                gil_disabled: false,
                                python_version: (
                                    3,
                                    12,
                                ),
                            },
                        ],
                        platform_tag: [
                            Manylinux {
                                major: 2,
                                minor: 17,
                                arch: X86_64,
                            },
                            Manylinux2014 {
                                arch: X86_64,
                            },
                        ],
                        variant: None,
                        repr: "cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64",
                    },
                },
            ],
        )
        "#);
    }

    #[test]
    fn test_parse_unknown_expanded_tag() {
        let tags = ExpandedTags::parse(vec!["py3-foo-any"]).unwrap();

        insta::assert_debug_snapshot!(tags, @r#"
        ExpandedTags(
            [
                Large {
                    large: WheelTagLarge {
                        build_tag: None,
                        python_tag: [
                            Python {
                                major: 3,
                                minor: None,
                            },
                        ],
                        abi_tag: [],
                        platform_tag: [
                            Any,
                        ],
                        variant: None,
                        repr: "py3-foo-any",
                    },
                },
            ],
        )
        "#);
    }

    #[test]
    fn test_parse_expanded_tag_with_dots() {
        let tags = ExpandedTags::parse(vec!["py2.py3-none-any"]).unwrap();

        insta::assert_debug_snapshot!(tags, @r#"
        ExpandedTags(
            [
                Large {
                    large: WheelTagLarge {
                        build_tag: None,
                        python_tag: [
                            Python {
                                major: 2,
                                minor: None,
                            },
                            Python {
                                major: 3,
                                minor: None,
                            },
                        ],
                        abi_tag: [
                            None,
                        ],
                        platform_tag: [
                            Any,
                        ],
                        variant: None,
                        repr: "py2.py3-none-any",
                    },
                },
            ],
        )
        "#);
    }

    #[test]
    fn test_error_missing_language_tag() {
        let err = ExpandedTags::parse(vec![""]).unwrap_err();
        insta::assert_debug_snapshot!(err, @r#"
        MissingLanguageTag(
            "",
        )
        "#);
    }

    #[test]
    fn test_error_missing_abi_tag() {
        let err = ExpandedTags::parse(vec!["py3"]).unwrap_err();
        insta::assert_debug_snapshot!(err, @r#"
        MissingAbiTag(
            "py3",
        )
        "#);
    }

    #[test]
    fn test_error_missing_platform_tag() {
        let err = ExpandedTags::parse(vec!["py3-none"]).unwrap_err();
        insta::assert_debug_snapshot!(err, @r#"
        MissingPlatformTag(
            "py3-none",
        )
        "#);
    }

    #[test]
    fn test_parse_expanded_tag_single_segment() {
        let result = parse_expanded_tag("py3-none-any");
        assert!(result.is_ok());
        let tag = result.unwrap();

        insta::assert_debug_snapshot!(tag, @r"
        Small {
            small: WheelTagSmall {
                python_tag: Python {
                    major: 3,
                    minor: None,
                },
                abi_tag: None,
                platform_tag: Any,
            },
        }
        ");
    }

    #[test]
    fn test_parse_expanded_tag_multi_segment() {
        let result = parse_expanded_tag("cp39.cp310-cp39.cp310-linux_x86_64.linux_i686");
        assert!(result.is_ok());
        let tag = result.unwrap();

        insta::assert_debug_snapshot!(tag, @r#"
        Large {
            large: WheelTagLarge {
                build_tag: None,
                python_tag: [
                    CPython {
                        python_version: (
                            3,
                            9,
                        ),
                    },
                    CPython {
                        python_version: (
                            3,
                            10,
                        ),
                    },
                ],
                abi_tag: [
                    CPython {
                        gil_disabled: false,
                        python_version: (
                            3,
                            9,
                        ),
                    },
                    CPython {
                        gil_disabled: false,
                        python_version: (
                            3,
                            10,
                        ),
                    },
                ],
                platform_tag: [
                    Linux {
                        arch: X86_64,
                    },
                    Linux {
                        arch: X86,
                    },
                ],
                variant: None,
                repr: "cp39.cp310-cp39.cp310-linux_x86_64.linux_i686",
            },
        }
        "#);
    }

    #[test]
    fn test_parse_expanded_tag_empty() {
        let result = parse_expanded_tag("");
        assert!(result.is_err());

        insta::assert_debug_snapshot!(result.unwrap_err(), @r#"
        MissingLanguageTag(
            "",
        )
        "#);
    }

    #[test]
    fn test_parse_expanded_tag_one_segment() {
        let result = parse_expanded_tag("python");
        assert!(result.is_err());

        insta::assert_debug_snapshot!(result.unwrap_err(), @r#"
        MissingAbiTag(
            "python",
        )
        "#);
    }

    #[test]
    fn test_parse_expanded_tag_two_segments() {
        let result = parse_expanded_tag("py3-none");
        assert!(result.is_err());

        insta::assert_debug_snapshot!(result.unwrap_err(), @r#"
        MissingPlatformTag(
            "py3-none",
        )
        "#);
    }

    #[test]
    fn test_expanded_tags_ordering() {
        let tags1 = ExpandedTags::parse(vec!["py3-none-any"]).unwrap();
        let tags2 = ExpandedTags::parse(vec!["py3-none-any"]).unwrap();
        let tags3 = ExpandedTags::parse(vec!["py2-none-any"]).unwrap();

        assert_eq!(tags1, tags2);
        assert_ne!(tags1, tags3);
    }
}

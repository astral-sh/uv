use std::str::FromStr;

use thiserror::Error;

use uv_platform_tags::{
    AbiTag, LanguageTag, ParseAbiTagError, ParseLanguageTagError, ParsePlatformTagError,
    PlatformTag, Tags,
};

use crate::wheel_tag::WheelTagSmall;

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
pub struct ExpandedTags(Box<[WheelTagSmall]>);

impl ExpandedTags {
    /// Parse a list of expanded wheel tags (e.g., `py3-none-any`).
    pub fn parse<'a>(tags: impl IntoIterator<Item = &'a str>) -> Result<Self, ExpandedTagError> {
        let tags = tags
            .into_iter()
            .map(parse_expanded_tag)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self(tags.into_boxed_slice()))
    }

    /// Returns `true` if the wheel is compatible with the given tags.
    pub fn is_compatible(&self, compatible_tags: &Tags) -> bool {
        self.0.iter().any(|tag| {
            compatible_tags.is_compatible(
                std::slice::from_ref(&tag.python_tag),
                std::slice::from_ref(&tag.abi_tag),
                std::slice::from_ref(&tag.platform_tag),
            )
        })
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
}

/// Parse an expanded (i.e., simplified) wheel tag, e.g. `py3-none-any`.
///
/// Unlike parsing tags in a wheel filename, each tag in this case is expected to contain exactly
/// three segments separated by `-`: a language tag, an ABI tag, and a platform tag.
fn parse_expanded_tag(tag: &str) -> Result<WheelTagSmall, ExpandedTagError> {
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
    if splitter.next().is_some() {
        return Err(ExpandedTagError::ExtraSegment(tag.to_string()));
    }
    let python_tag = LanguageTag::from_str(&tag[..python_tag_index])
        .map_err(|err| ExpandedTagError::InvalidLanguageTag(tag.to_string(), err))?;
    let abi_tag = AbiTag::from_str(&tag[python_tag_index + 1..abi_tag_index])
        .map_err(|err| ExpandedTagError::InvalidAbiTag(tag.to_string(), err))?;
    let platform_tag = PlatformTag::from_str(&tag[abi_tag_index + 1..])
        .map_err(|err| ExpandedTagError::InvalidPlatformTag(tag.to_string(), err))?;
    Ok(WheelTagSmall {
        python_tag,
        abi_tag,
        platform_tag,
    })
}

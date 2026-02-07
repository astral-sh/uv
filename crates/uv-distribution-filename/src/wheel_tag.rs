use std::fmt::{Display, Formatter};

use crate::BuildTag;
use uv_platform_tags::{AbiTag, LanguageTag, PlatformTag};
use uv_small_str::SmallString;

/// A [`SmallVec`] type for storing tags.
///
/// Wheels tend to include a single language, ABI, and platform tag, so we use a [`SmallVec`] with a
/// capacity of 1 to optimize for this common case.
pub(crate) type TagSet<T> = smallvec::SmallVec<[T; 3]>;

/// The portion of the wheel filename following the name and version: the optional build tag, along
/// with the Python tag(s), ABI tag(s), and platform tag(s).
///
/// Most wheels consist of a single Python, ABI, and platform tag (and no build tag). We represent
/// such wheels with [`WheelTagSmall`], a variant with a smaller memory footprint and (generally)
/// zero allocations. The [`WheelTagLarge`] variant is used for wheels with multiple tags, a build
/// tag, or an unsupported tag (i.e., a tag that can't be represented by [`LanguageTag`],
/// [`AbiTag`], or [`PlatformTag`]). (Unsupported tags are filtered out, but retained in the display
/// representation of [`WheelTagLarge`].)
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
pub(crate) enum WheelTag {
    Small { small: WheelTagSmall },
    Large { large: Box<WheelTagLarge> },
}

impl WheelTag {
    /// Return the Python tags.
    pub(crate) fn python_tags(&self) -> &[LanguageTag] {
        match self {
            Self::Small { small } => std::slice::from_ref(&small.python_tag),
            Self::Large { large } => large.python_tag.as_slice(),
        }
    }

    /// Return the ABI tags.
    pub(crate) fn abi_tags(&self) -> &[AbiTag] {
        match self {
            Self::Small { small } => std::slice::from_ref(&small.abi_tag),
            Self::Large { large } => large.abi_tag.as_slice(),
        }
    }

    /// Return the platform tags.
    pub(crate) fn platform_tags(&self) -> &[PlatformTag] {
        match self {
            Self::Small { small } => std::slice::from_ref(&small.platform_tag),
            Self::Large { large } => large.platform_tag.as_slice(),
        }
    }

    /// Return the build tag, if present.
    pub(crate) fn build_tag(&self) -> Option<&BuildTag> {
        match self {
            Self::Small { .. } => None,
            Self::Large { large } => large.build_tag.as_ref(),
        }
    }

    /// Check if the compressed tag sets are sorted per PEP 425.
    ///
    /// PEP 425 states that compressed tag sets (`.`-separated) should be sorted.
    /// For example, `manylinux_2_17_x86_64.manylinux2014_x86_64` is unsorted and
    /// should be `manylinux2014_x86_64.manylinux_2_17_x86_64`.
    ///
    /// See: <https://github.com/pypi/warehouse/issues/18129>
    pub(crate) fn has_sorted_tags(&self) -> bool {
        match self {
            // Single tags are always sorted.
            Self::Small { .. } => true,
            Self::Large { large } => large.has_sorted_tags(),
        }
    }
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
#[rkyv(derive(Debug), attr(expect(clippy::struct_field_names)))]
#[expect(clippy::struct_field_names)]
pub(crate) struct WheelTagSmall {
    /// The Python tag, e.g., `py3` in `1.2.3-py3-none-any`.
    pub(crate) python_tag: LanguageTag,
    /// The ABI tag, e.g., `none` in `1.2.3-py3-none-any`.
    pub(crate) abi_tag: AbiTag,
    /// The platform tag, e.g., `none` in `1.2.3-py3-none-any`.
    pub(crate) platform_tag: PlatformTag,
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
pub(crate) struct WheelTagLarge {
    /// The optional build tag, e.g., `73` in `1.2.3-73-py3-none-any`.
    pub(crate) build_tag: Option<BuildTag>,
    /// The Python tag(s), e.g., `py3` in `1.2.3-73-py3-none-any`.
    pub(crate) python_tag: TagSet<LanguageTag>,
    /// The ABI tag(s), e.g., `none` in `1.2.3-73-py3-none-any`.
    pub(crate) abi_tag: TagSet<AbiTag>,
    /// The platform tag(s), e.g., `none` in `1.2.3-73-py3-none-any`.
    pub(crate) platform_tag: TagSet<PlatformTag>,
    /// The string representation of the tag.
    ///
    /// Preserves any unsupported tags that were filtered out when parsing the wheel filename.
    pub(crate) repr: SmallString,
}

impl WheelTagLarge {
    /// Check if the compressed tag sets are sorted per PEP 425.
    fn has_sorted_tags(&self) -> bool {
        // The repr format is: [build_tag-]python_tag-abi_tag-platform_tag
        // where each tag component can be a `.`-separated list of tags.
        //
        // We need to check that each `.`-separated list is sorted.
        let repr: &str = &self.repr;

        // Skip the build tag if present.
        let tag_part = if self.build_tag.is_some() {
            // Build tag is the first `-` separated component.
            repr.split_once('-').map_or(repr, |(_, rest)| rest)
        } else {
            repr
        };

        // Split into python-abi-platform components.
        let mut parts = tag_part.splitn(3, '-');
        let python_tags = parts.next();
        let abi_tags = parts.next();
        let platform_tags = parts.next();

        // Check each component for sorted order.
        for tags in [python_tags, abi_tags, platform_tags].into_iter().flatten() {
            if !is_sorted_tag_set(tags) {
                return false;
            }
        }

        true
    }
}

/// Check if a `.`-separated tag set is sorted.
fn is_sorted_tag_set(tags: &str) -> bool {
    let mut prev: Option<&str> = None;
    for tag in tags.split('.') {
        if let Some(p) = prev {
            if tag < p {
                return false;
            }
        }
        prev = Some(tag);
    }
    true
}

impl Display for WheelTagLarge {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.repr)
    }
}

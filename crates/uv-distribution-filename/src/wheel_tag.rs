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
#[allow(clippy::struct_field_names)]
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

impl Display for WheelTagLarge {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.repr)
    }
}

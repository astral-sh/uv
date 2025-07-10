pub use abi_tag::{AbiTag, ParseAbiTagError};
pub use language_tag::{LanguageTag, ParseLanguageTagError};
pub use platform::{Arch, Os, Platform, PlatformError};
pub use platform_tag::{ParsePlatformTagError, PlatformTag};
pub use tags::{BinaryFormat, IncompatibleTag, TagCompatibility, TagPriority, Tags, TagsError};

mod abi_tag;
mod language_tag;
mod platform;
mod platform_tag;
mod tags;

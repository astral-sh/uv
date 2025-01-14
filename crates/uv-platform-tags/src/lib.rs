pub use abi_tag::AbiTag;
pub use language_tag::LanguageTag;
pub use platform::{Arch, Os, Platform, PlatformError};
pub use tags::{IncompatibleTag, TagCompatibility, TagPriority, Tags, TagsError};

mod abi_tag;
mod language_tag;
mod platform;
mod tags;

pub use platform::{Arch, Os, Platform, PlatformError};
pub use tags::{IncompatibleTag, TagCompatibility, TagPriority, Tags, TagsError};

mod platform;
mod tags;

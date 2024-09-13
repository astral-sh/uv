use platform_tags::Tags;

#[derive(Debug, Clone)]
pub enum TagPolicy {
    /// Exclusively consider wheels that match the specified platform tags.
    Required(Tags),
    /// Prefer wheels that match the specified platform tags, but fall back to incompatible wheels
    /// if  necessary.
    Preferred(Tags),
}

impl TagPolicy {
    /// Returns the platform tags to consider.
    pub const fn tags(&self) -> &Tags {
        match self {
            TagPolicy::Required(tags) | TagPolicy::Preferred(tags) => tags,
        }
    }

    #[must_use]
    pub fn into_tags(self) -> Tags {
        match self {
            TagPolicy::Required(tags) | TagPolicy::Preferred(tags) => tags,
        }
    }

    pub fn is_required(&self) -> bool {
        matches!(self, TagPolicy::Required(_))
    }
}

// #[derive(Debug, Copy, Clone)]
// pub enum TagPolicy<'tags> {
//     /// Exclusively consider wheels that match the specified platform tags.
//     Required(&'tags Tags),
//     /// Prefer wheels that match the specified platform tags, but fall back to incompatible wheels
//     /// if  necessary.
//     Preferred(&'tags Tags),
// }
//
// impl<'tags> TagPolicy<'tags> {
//     /// Returns the platform tags to consider.
//     pub(crate) const fn tags(&self) -> &'tags Tags {
//         match self {
//             TagPolicy::Required(tags) | TagPolicy::Preferred(tags) => tags,
//         }
//     }
// }

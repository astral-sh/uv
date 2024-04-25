use std::path::Path;

/// Like FromStr, but with information about where the str came from
pub trait TrackedFromStr: Sized {
    /// The associated error which can be returned from parsing.
    type Err;

    /// Convert from input with optional source and working_dir
    fn tracked_from_str(
        input: &str,
        source: Option<&Path>,
        working_dir: Option<&Path>,
    ) -> Result<Self, Self::Err>;
}

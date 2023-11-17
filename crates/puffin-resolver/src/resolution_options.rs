use crate::{PreReleaseMode, ResolutionMode};
use chrono::{DateTime, Utc};

/// Options for resolving a manifest.
#[derive(Debug, Default, Copy, Clone)]
pub struct ResolutionOptions {
    pub(crate) resolution_mode: ResolutionMode,
    pub(crate) prerelease_mode: PreReleaseMode,
    pub(crate) exclude_newer: Option<DateTime<Utc>>,
}

impl ResolutionOptions {
    pub fn new(
        resolution_mode: ResolutionMode,
        prerelease_mode: PreReleaseMode,
        exclude_newer: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            resolution_mode,
            prerelease_mode,
            exclude_newer,
        }
    }
}

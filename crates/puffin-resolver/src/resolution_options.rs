use chrono::{DateTime, Utc};

use crate::{PreReleaseMode, ResolutionMode};

/// Options for resolving a manifest.
#[derive(Debug, Default, Copy, Clone)]
pub struct ResolutionOptions {
    pub resolution_mode: ResolutionMode,
    pub prerelease_mode: PreReleaseMode,
    pub exclude_newer: Option<DateTime<Utc>>,
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

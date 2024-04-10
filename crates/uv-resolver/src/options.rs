use chrono::{DateTime, Utc};

use crate::hash_checking_mode::HashCheckingMode;
use crate::{DependencyMode, PreReleaseMode, ResolutionMode};

/// Options for resolving a manifest.
#[derive(Debug, Default, Copy, Clone)]
pub struct Options {
    pub resolution_mode: ResolutionMode,
    pub prerelease_mode: PreReleaseMode,
    pub dependency_mode: DependencyMode,
    pub hash_checking_mode: HashCheckingMode,
    pub exclude_newer: Option<DateTime<Utc>>,
}

/// Builder for [`Options`].
#[derive(Debug, Default, Clone)]
pub struct OptionsBuilder {
    resolution_mode: ResolutionMode,
    prerelease_mode: PreReleaseMode,
    dependency_mode: DependencyMode,
    hash_checking_mode: HashCheckingMode,
    exclude_newer: Option<DateTime<Utc>>,
}

impl OptionsBuilder {
    /// Creates a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the [`ResolutionMode`].
    #[must_use]
    pub fn resolution_mode(mut self, resolution_mode: ResolutionMode) -> Self {
        self.resolution_mode = resolution_mode;
        self
    }

    /// Sets the [`PreReleaseMode`].
    #[must_use]
    pub fn prerelease_mode(mut self, prerelease_mode: PreReleaseMode) -> Self {
        self.prerelease_mode = prerelease_mode;
        self
    }

    /// Sets the dependency mode.
    #[must_use]
    pub fn dependency_mode(mut self, dependency_mode: DependencyMode) -> Self {
        self.dependency_mode = dependency_mode;
        self
    }

    /// Sets the hash-checking mode.
    #[must_use]
    pub fn hash_checking_mode(mut self, hash_checking_mode: HashCheckingMode) -> Self {
        self.hash_checking_mode = hash_checking_mode;
        self
    }

    /// Sets the exclusion date.
    #[must_use]
    pub fn exclude_newer(mut self, exclude_newer: Option<DateTime<Utc>>) -> Self {
        self.exclude_newer = exclude_newer;
        self
    }

    /// Builds the options.
    pub fn build(self) -> Options {
        Options {
            resolution_mode: self.resolution_mode,
            prerelease_mode: self.prerelease_mode,
            dependency_mode: self.dependency_mode,
            hash_checking_mode: self.hash_checking_mode,
            exclude_newer: self.exclude_newer,
        }
    }
}

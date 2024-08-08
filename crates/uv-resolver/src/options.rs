use uv_configuration::IndexStrategy;

use crate::{DependencyMode, ExcludeNewer, PrereleaseMode, ResolutionMode};

/// Options for resolving a manifest.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub struct Options {
    pub resolution_mode: ResolutionMode,
    pub prerelease_mode: PrereleaseMode,
    pub dependency_mode: DependencyMode,
    pub exclude_newer: Option<ExcludeNewer>,
    pub index_strategy: IndexStrategy,
}

/// Builder for [`Options`].
#[derive(Debug, Default, Clone)]
pub struct OptionsBuilder {
    resolution_mode: ResolutionMode,
    prerelease_mode: PrereleaseMode,
    dependency_mode: DependencyMode,
    exclude_newer: Option<ExcludeNewer>,
    index_strategy: IndexStrategy,
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

    /// Sets the [`PrereleaseMode`].
    #[must_use]
    pub fn prerelease_mode(mut self, prerelease_mode: PrereleaseMode) -> Self {
        self.prerelease_mode = prerelease_mode;
        self
    }

    /// Sets the dependency mode.
    #[must_use]
    pub fn dependency_mode(mut self, dependency_mode: DependencyMode) -> Self {
        self.dependency_mode = dependency_mode;
        self
    }

    /// Sets the exclusion date.
    #[must_use]
    pub fn exclude_newer(mut self, exclude_newer: Option<ExcludeNewer>) -> Self {
        self.exclude_newer = exclude_newer;
        self
    }

    /// Sets the index strategy.
    #[must_use]
    pub fn index_strategy(mut self, index_strategy: IndexStrategy) -> Self {
        self.index_strategy = index_strategy;
        self
    }

    /// Builds the options.
    pub fn build(self) -> Options {
        Options {
            resolution_mode: self.resolution_mode,
            prerelease_mode: self.prerelease_mode,
            dependency_mode: self.dependency_mode,
            exclude_newer: self.exclude_newer,
            index_strategy: self.index_strategy,
        }
    }
}

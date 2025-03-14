use crate::fork_strategy::ForkStrategy;
use crate::{DependencyMode, ExcludeNewer, PrereleaseMode, ResolutionMode};
use uv_configuration::{BuildOptions, IndexStrategy};
use uv_pypi_types::SupportedEnvironments;

/// Options for resolving a manifest.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Options {
    pub resolution_mode: ResolutionMode,
    pub prerelease_mode: PrereleaseMode,
    pub dependency_mode: DependencyMode,
    pub fork_strategy: ForkStrategy,
    pub exclude_newer: Option<ExcludeNewer>,
    pub index_strategy: IndexStrategy,
    pub required_environments: SupportedEnvironments,
    pub flexibility: Flexibility,
    pub build_options: BuildOptions,
}

/// Builder for [`Options`].
#[derive(Debug, Default, Clone)]
pub struct OptionsBuilder {
    resolution_mode: ResolutionMode,
    prerelease_mode: PrereleaseMode,
    dependency_mode: DependencyMode,
    fork_strategy: ForkStrategy,
    exclude_newer: Option<ExcludeNewer>,
    index_strategy: IndexStrategy,
    required_environments: SupportedEnvironments,
    flexibility: Flexibility,
    build_options: BuildOptions,
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

    /// Sets the multi-version mode.
    #[must_use]
    pub fn fork_strategy(mut self, fork_strategy: ForkStrategy) -> Self {
        self.fork_strategy = fork_strategy;
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

    /// Sets the required platforms.
    #[must_use]
    pub fn required_environments(mut self, required_environments: SupportedEnvironments) -> Self {
        self.required_environments = required_environments;
        self
    }

    /// Sets the [`Flexibility`].
    #[must_use]
    pub fn flexibility(mut self, flexibility: Flexibility) -> Self {
        self.flexibility = flexibility;
        self
    }

    /// Sets the [`BuildOptions`].
    #[must_use]
    pub fn build_options(mut self, build_options: BuildOptions) -> Self {
        self.build_options = build_options;
        self
    }

    /// Builds the options.
    pub fn build(self) -> Options {
        Options {
            resolution_mode: self.resolution_mode,
            prerelease_mode: self.prerelease_mode,
            dependency_mode: self.dependency_mode,
            fork_strategy: self.fork_strategy,
            exclude_newer: self.exclude_newer,
            index_strategy: self.index_strategy,
            required_environments: self.required_environments,
            flexibility: self.flexibility,
            build_options: self.build_options,
        }
    }
}

/// Whether the [`Options`] are configurable or fixed.
///
/// Applies to the [`ResolutionMode`], [`PrereleaseMode`], and [`DependencyMode`] fields.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Flexibility {
    /// The setting is configurable.
    #[default]
    Configurable,
    /// The setting is fixed.
    Fixed,
}
